use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};

use ffmpeg::{
    codec, format, frame, media, software::resampling::Context as Resampler,
    util::format::sample::Type as SampleType,
};
use ffmpeg_next as ffmpeg;

use ringbuf::{
    HeapRb,
    traits::{Consumer, Observer, Producer, Split},
};

use serde::Serialize;

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const CHANNELS: usize = 2;
const BUFFER_SECONDS: usize = 2;
const DECODE_CHUNK: usize = 4096;

type RingProducer = ringbuf::HeapProd<f32>;
type RingConsumer = ringbuf::HeapCons<f32>;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerState {
    pub status: PlaybackStatus,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,
    pub current_path: String,

    #[serde(skip)]
    cursor: u64,

    #[serde(skip)]
    sample_rate: u32,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Stopped,
            position: 0.0,
            duration: 0.0,
            volume: 0.65,
            current_path: String::new(),
            cursor: 0,
            sample_rate: 0,
        }
    }
}

enum DecoderCommand {
    Seek {
        seconds: f64,
        reply: Sender<Result<u64, String>>,
    },
    Queue(AudioDecoder),
    Shutdown,
}

struct TrackBoundary {
    frame: u64,
    duration: f64,
    path: String,
}

struct SharedAudio {
    consumer: Option<RingConsumer>,
    eof: bool,
    boundaries: VecDeque<TrackBoundary>,
    consumed: u64,
}

pub struct AudioPlayer {
    state: Arc<Mutex<PlayerState>>,
    shared: Arc<Mutex<SharedAudio>>,
    stream: Mutex<Option<Stream>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    commands: Mutex<Option<Sender<DecoderCommand>>>,
    alive: Arc<AtomicBool>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(PlayerState::default())),
            shared: Arc::new(Mutex::new(SharedAudio {
                consumer: None,
                eof: false,
                boundaries: VecDeque::new(),
                consumed: 0,
            })),
            stream: Mutex::new(None),
            worker: Mutex::new(None),
            commands: Mutex::new(None),
            alive: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn get_state(&self) -> Result<PlayerState, String> {
        Ok(self.state.lock().map_err(|e| e.to_string())?.clone())
    }

    fn shutdown(&self) -> Result<(), String> {
        self.alive.store(false, Ordering::Release);

        self.stream.lock().map_err(|e| e.to_string())?.take();

        if let Some(tx) = self.commands.lock().map_err(|e| e.to_string())?.take() {
            let _ = tx.send(DecoderCommand::Shutdown);
        }

        if let Some(handle) = self.worker.lock().map_err(|e| e.to_string())?.take() {
            let _ = handle.join();
        }

        let mut shared = self.shared.lock().map_err(|e| e.to_string())?;
        shared.consumer = None;
        shared.eof = false;
        shared.boundaries.clear();
        shared.consumed = 0;

        Ok(())
    }

    pub fn load(&self, path: &str) -> Result<(), String> {
        self.shutdown()?;

        ffmpeg::init().map_err(|e| e.to_string())?;

        let device = cpal::default_host()
            .default_output_device()
            .ok_or("No audio output device found")?;

        let supported = device.default_output_config().map_err(|e| e.to_string())?;

        let format = supported.sample_format();
        let config: StreamConfig = supported.into();

        if config.channels == 0 || config.sample_rate == 0 {
            return Err("Invalid output configuration".into());
        }

        let sample_rate = config.sample_rate;

        let decoder = AudioDecoder::open(path, sample_rate)?;

        let duration = decoder.duration;

        let capacity = sample_rate as usize * CHANNELS * BUFFER_SECONDS;

        let (producer, consumer) = HeapRb::<f32>::new(capacity).split();

        {
            let mut shared = self.shared.lock().map_err(|e| e.to_string())?;
            shared.consumer = Some(consumer);
            shared.eof = false;
            shared.boundaries.clear();
            shared.consumed = 0;
        }

        {
            let mut state = self.state.lock().map_err(|e| e.to_string())?;

            state.status = PlaybackStatus::Stopped;
            state.position = 0.0;
            state.duration = duration;
            state.current_path = path.to_owned();
            state.cursor = 0;
            state.sample_rate = sample_rate;
        }

        let (tx, rx) = mpsc::channel();

        self.alive.store(true, Ordering::Release);

        let alive = Arc::clone(&self.alive);
        let shared = Arc::clone(&self.shared);
        let state = Arc::clone(&self.state);

        let handle = thread::spawn(move || {
            decoder_worker(decoder, producer, capacity, rx, shared, state, alive);
        });

        *self.worker.lock().map_err(|e| e.to_string())? = Some(handle);
        *self.commands.lock().map_err(|e| e.to_string())? = Some(tx);

        let channels = config.channels as usize;

        let stream_result = match format {
            SampleFormat::F32 => {
                let shared = Arc::clone(&self.shared);
                let state = Arc::clone(&self.state);

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [f32], _| {
                        render(data, channels, &shared, &state, |x| x.clamp(-1.0, 1.0));
                    },
                    |e| eprintln!("CPAL error: {e}"),
                    None,
                )
            }

            SampleFormat::I16 => {
                let shared = Arc::clone(&self.shared);
                let state = Arc::clone(&self.state);

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [i16], _| {
                        render(data, channels, &shared, &state, |x| {
                            (x.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                        });
                    },
                    |e| eprintln!("CPAL error: {e}"),
                    None,
                )
            }

            SampleFormat::U16 => {
                let shared = Arc::clone(&self.shared);
                let state = Arc::clone(&self.state);

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [u16], _| {
                        render(data, channels, &shared, &state, |x| {
                            ((x.clamp(-1.0, 1.0) + 1.0) * 0.5 * u16::MAX as f32) as u16
                        });
                    },
                    |e| eprintln!("CPAL error: {e}"),
                    None,
                )
            }

            other => {
                self.shutdown()?;
                return Err(format!("Unsupported sample format: {other:?}"));
            }
        };

        match stream_result {
            Ok(stream) => {
                *self.stream.lock().map_err(|e| e.to_string())? = Some(stream);
                Ok(())
            }
            Err(e) => {
                self.shutdown()?;
                Err(e.to_string())
            }
        }
    }

    pub fn gapless_queue(&self, path: &str) -> Result<(), String> {
        let rate = self.state.lock().map_err(|e| e.to_string())?.sample_rate;
        if rate == 0 {
            return Err("Load a track first".into());
        }
        let next = AudioDecoder::open(path, rate)?;
        let commands = self.commands.lock().map_err(|e| e.to_string())?;
        commands
            .as_ref()
            .ok_or("Decoder is not running")?
            .send(DecoderCommand::Queue(next))
            .map_err(|e| e.to_string())
    }

    pub fn play(&self) -> Result<(), String> {
        let stream_slot = self.stream.lock().map_err(|e| e.to_string())?;
        let stream = stream_slot.as_ref().ok_or("Load a track first")?;

        let restart = {
            let state = self.state.lock().map_err(|e| e.to_string())?;
            state.duration > 0.0 && state.position >= state.duration
        };

        if restart {
            self.seek(0.0)?;
        }
        stream.play().map_err(|e| e.to_string())?;

        self.state.lock().map_err(|e| e.to_string())?.status = PlaybackStatus::Playing;

        Ok(())
    }

    pub fn pause(&self) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;

        if state.status == PlaybackStatus::Playing {
            state.status = PlaybackStatus::Paused;
        }

        Ok(())
    }

    pub fn resume(&self) -> Result<(), String> {
        self.play()
    }

    pub fn stop(&self) -> Result<(), String> {
        self.seek(0.0)?;

        self.state.lock().map_err(|e| e.to_string())?.status = PlaybackStatus::Stopped;

        Ok(())
    }

    pub fn set_volume(&self, volume: f32) -> Result<(), String> {
        if !volume.is_finite() {
            return Err("Invalid volume".into());
        }

        self.state.lock().map_err(|e| e.to_string())?.volume = volume.clamp(0.0, 1.0);

        Ok(())
    }

    pub fn seek(&self, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() {
            return Err("Invalid seek position".into());
        }

        let duration = {
            let state = self.state.lock().map_err(|e| e.to_string())?;

            if state.sample_rate == 0 {
                return Err("No track loaded".into());
            }

            state.duration
        };

        let target = seconds.clamp(0.0, duration);

        let commands = self.commands.lock().map_err(|e| e.to_string())?;

        let tx = commands.as_ref().ok_or("Decoder is not running")?;

        let (reply_tx, reply_rx) = mpsc::channel();

        tx.send(DecoderCommand::Seek {
            seconds: target,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

        reply_rx.recv().map_err(|e| e.to_string())??;

        Ok(())
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

// ---------------------------------------------------------
// CPAL callback
// ---------------------------------------------------------

fn render<T: Copy>(
    data: &mut [T],
    channels: usize,
    shared: &Arc<Mutex<SharedAudio>>,
    state: &Arc<Mutex<PlayerState>>,
    mut convert: impl FnMut(f32) -> T,
) {
    let silence = convert(0.0);
    data.fill(silence);
    let Ok(mut audio) = shared.try_lock() else {
        return;
    };

    let Ok(mut player) = state.try_lock() else {
        return;
    };

    if player.status != PlaybackStatus::Playing {
        return;
    }

    let volume = player.volume;

    let mut rendered_frames = 0u64;
    let mut reached_eof = false;
    let mut current_frames = 0u64;

    for output in data.chunks_mut(channels) {
        if audio
            .boundaries
            .front()
            .map_or(false, |boundary| audio.consumed == boundary.frame)
        {
            let (next_duration, next_path) = {
                let boundary = audio.boundaries.front().unwrap();
                (boundary.duration, boundary.path.clone())
            };
            player.cursor += current_frames;
            current_frames = 0;
            player.cursor = 0;
            player.position = 0.0;
            player.duration = next_duration;
            player.current_path = next_path;
            audio.boundaries.pop_front();
        }

        let samples = {
            let Some(consumer) = audio.consumer.as_mut() else {
                break;
            };
            if consumer.occupied_len() < CHANNELS {
                None
            } else {
                Some((
                    consumer.try_pop().unwrap_or(0.0),
                    consumer.try_pop().unwrap_or(0.0),
                ))
            }
        };
        let Some((left, right)) = samples else {
            if audio.eof {
                player.status = PlaybackStatus::Stopped;
                reached_eof = true;
            }
            break;
        };

        for (channel, sample) in output.iter_mut().enumerate() {
            let value = if channels == 1 {
                (left + right) * 0.5
            } else {
                match channel {
                    0 => left,
                    1 => right,
                    _ => 0.0,
                }
            };

            *sample = convert(value * volume);
        }

        rendered_frames += 1;
        audio.consumed += 1;
        current_frames += 1;
        if audio
            .boundaries
            .front()
            .map_or(false, |boundary| audio.consumed == boundary.frame)
        {
            let (next_duration, next_path) = {
                let boundary = audio.boundaries.front().unwrap();
                (boundary.duration, boundary.path.clone())
            };
            player.cursor += current_frames;
            current_frames = 0;
            player.cursor = 0;
            player.position = 0.0;
            player.duration = next_duration;
            player.current_path = next_path;
            audio.boundaries.pop_front();
        }
    }

    player.cursor += current_frames;
    if reached_eof {
        player.position = player.duration;
    } else if rendered_frames != 0 {
        player.position = (player.cursor as f64 / player.sample_rate as f64).min(player.duration);
    }
}

// ---------------------------------------------------------
// Decoder worker
// ---------------------------------------------------------

fn decoder_worker(
    mut decoder: AudioDecoder,
    mut producer: RingProducer,
    capacity: usize,
    rx: Receiver<DecoderCommand>,
    shared: Arc<Mutex<SharedAudio>>,
    state: Arc<Mutex<PlayerState>>,
    alive: Arc<AtomicBool>,
) {
    let mut pending = Vec::<f32>::new();
    let mut pending_offset = 0usize;
    let mut eof = false;
    let mut queued: Option<AudioDecoder> = None;
    let mut produced = 0u64;

    while alive.load(Ordering::Acquire) {
        loop {
            match rx.try_recv() {
                Ok(DecoderCommand::Shutdown) => return,
                Ok(DecoderCommand::Queue(next)) => {
                    queued = Some(next);
                }

                Ok(DecoderCommand::Seek { seconds, reply }) => {
                    let result = decoder.seek(seconds);

                    if let Ok(cursor) = result {
                        let (new_producer, new_consumer) = HeapRb::<f32>::new(capacity).split();

                        let mut audio = shared.lock().unwrap();
                        let mut player = state.lock().unwrap();

                        producer = new_producer;
                        audio.consumer = Some(new_consumer);
                        audio.eof = false;
                        audio.boundaries.clear();
                        audio.consumed = 0;
                        produced = 0;

                        pending.clear();
                        pending_offset = 0;
                        eof = false;

                        player.cursor = cursor;
                        player.position = cursor as f64 / player.sample_rate as f64;

                        let _ = reply.send(Ok(cursor));
                    } else {
                        let _ = reply.send(result);
                    }
                }

                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        if eof {
            if let Some(next) = queued.take() {
                let mut audio = shared.lock().unwrap();
                audio.boundaries.push_back(TrackBoundary {
                    frame: produced,
                    duration: next.duration,
                    path: next.path.clone(),
                });
                audio.eof = false;
                drop(audio);
                decoder = next;
                eof = false;
            } else {
                thread::sleep(Duration::from_millis(2));
                continue;
            }
        }

        if pending_offset < pending.len() {
            let free = producer.vacant_len();

            let available = pending.len() - pending_offset;
            let count = free.min(available) & !1;

            if count > 0 {
                let written = producer.push_slice(&pending[pending_offset..pending_offset + count]);

                pending_offset += written;
                produced += (written / CHANNELS) as u64;
            }

            if pending_offset == pending.len() {
                pending.clear();
                pending_offset = 0;
            } else {
                thread::sleep(Duration::from_millis(2));
            }

            continue;
        }

        if producer.vacant_len() < DECODE_CHUNK * CHANNELS {
            thread::sleep(Duration::from_millis(2));
            continue;
        }

        match decoder.next_pcm() {
            Ok(Some(samples)) => {
                pending = samples;
                pending_offset = 0;
            }

            Ok(None) => {
                if let Some(next) = queued.take() {
                    let mut audio = shared.lock().unwrap();
                    audio.boundaries.push_back(TrackBoundary {
                        frame: produced,
                        duration: next.duration,
                        path: next.path.clone(),
                    });
                    audio.eof = false;
                    drop(audio);
                    decoder = next;
                } else {
                    eof = true;
                    shared.lock().unwrap().eof = true;
                }
            }

            Err(e) => {
                eprintln!("Audio decoder error: {e}");

                eof = true;
                shared.lock().unwrap().eof = true;
            }
        }
    }
}

struct AudioDecoder {
    path: String,
    input: format::context::Input,
    decoder: codec::decoder::Audio,
    resampler: Resampler,

    stream_index: usize,
    time_base: ffmpeg::Rational,

    output_rate: u32,
    duration: f64,

    seek_target: Option<u64>,

    output_cursor: u64,

    draining: bool,
    finished: bool,
}

impl AudioDecoder {
    fn open(path: &str, output_rate: u32) -> Result<Self, String> {
        let input = format::input(path).map_err(|e| e.to_string())?;

        let stream = input
            .streams()
            .best(media::Type::Audio)
            .ok_or("No audio stream found")?;

        let stream_index = stream.index();
        let time_base = stream.time_base();

        let context = codec::context::Context::from_parameters(stream.parameters())
            .map_err(|e| e.to_string())?;

        let decoder = context.decoder().audio().map_err(|e| e.to_string())?;

        let resampler = Self::make_resampler(&decoder, output_rate)?;

        let duration = if stream.duration() > 0 {
            let tb = f64::from(time_base);

            stream.duration() as f64 * tb
        } else if input.duration() > 0 {
            input.duration() as f64 / ffmpeg::ffi::AV_TIME_BASE as f64
        } else {
            0.0
        };

        Ok(Self {
            path: path.to_owned(),
            input,
            decoder,
            resampler,
            stream_index,
            time_base,
            output_rate,
            duration,
            seek_target: None,
            output_cursor: 0,
            draining: false,
            finished: false,
        })
    }

    fn make_resampler(decoder: &codec::decoder::Audio, rate: u32) -> Result<Resampler, String> {
        Resampler::get(
            decoder.format(),
            decoder.channel_layout(),
            decoder.rate(),
            ffmpeg::format::Sample::F32(SampleType::Packed),
            ffmpeg::ChannelLayout::STEREO,
            rate,
        )
        .map_err(|e| e.to_string())
    }

    fn seek(&mut self, seconds: f64) -> Result<u64, String> {
        let target = seconds.clamp(0.0, self.duration);

        let target_frame = (target * self.output_rate as f64).round() as u64;

        let tb = f64::from(self.time_base);

        if tb <= 0.0 {
            return Err("Invalid audio stream time base".into());
        }

        let timestamp = (target / tb).floor() as i64;

        let result = unsafe {
            ffmpeg::ffi::avformat_seek_file(
                self.input.as_mut_ptr(),
                self.stream_index as i32,
                i64::MIN,
                timestamp,
                timestamp,
                ffmpeg::ffi::AVSEEK_FLAG_BACKWARD,
            )
        };

        if result < 0 {
            return Err(format!(
                "FFmpeg seek failed: {}",
                ffmpeg::Error::from(result)
            ));
        }

        self.decoder.flush();

        self.resampler = Self::make_resampler(&self.decoder, self.output_rate)?;

        self.seek_target = Some(target_frame);

        self.output_cursor = 0;

        self.draining = false;
        self.finished = false;

        Ok(target_frame)
    }

    fn next_pcm(&mut self) -> Result<Option<Vec<f32>>, String> {
        if self.finished {
            return Ok(None);
        }

        loop {
            let mut decoded = frame::Audio::empty();

            match self.decoder.receive_frame(&mut decoded) {
                Ok(()) => {
                    let samples = self.convert_frame(&decoded)?;

                    if !samples.is_empty() {
                        return Ok(Some(samples));
                    }

                    continue;
                }

                Err(ffmpeg::Error::Eof) => {
                    return self.finish_resampler();
                }

                Err(ffmpeg::Error::Other {
                    errno: ffmpeg::error::EAGAIN,
                }) => {}

                Err(e) => {
                    eprintln!("receive_frame FFmpeg error: {e:?}");
                    return Err(e.to_string());
                }
            }

            if self.draining {
                return self.finish_resampler();
            }

            let mut found = false;

            for (stream, packet) in self.input.packets() {
                if stream.index() != self.stream_index {
                    continue;
                }
                self.decoder
                    .send_packet(&packet)
                    .map_err(|e| e.to_string())?;

                found = true;
                break;
            }

            if !found {
                self.draining = true;

                self.decoder.send_eof().map_err(|e| e.to_string())?;
            }
        }
    }

    fn convert_frame(&mut self, decoded: &frame::Audio) -> Result<Vec<f32>, String> {
        if let Some(pts) = decoded.timestamp() {
            let seconds = pts as f64 * f64::from(self.time_base);

            self.output_cursor = (seconds.max(0.0) * self.output_rate as f64).round() as u64;
        }

        let mut output = frame::Audio::empty();

        self.resampler.run(decoded, &mut output).map_err(|e| {
            eprintln!("Resampler run failed: {e:?}");
            e.to_string()
        })?;

        if output.samples() == 0 {
            return Ok(Vec::new());
        }

        let bytes = output.data(0);

        let mut pcm = Vec::with_capacity(output.samples() * CHANNELS);

        for sample in bytes.chunks_exact(4).take(output.samples() * CHANNELS) {
            pcm.push(f32::from_ne_bytes([
                sample[0], sample[1], sample[2], sample[3],
            ]));
        }

        let frame_count = (pcm.len() / CHANNELS) as u64;
        let frame_start = self.output_cursor;
        let frame_end = frame_start + frame_count;

        self.output_cursor = frame_end;

        if let Some(target) = self.seek_target {
            if frame_end <= target {
                return Ok(Vec::new());
            }

            if frame_start < target {
                let discard = (target - frame_start) as usize;

                pcm.drain(..discard * CHANNELS);
            }

            self.seek_target = None;
        }

        Ok(pcm)
    }

    fn finish_resampler(&mut self) -> Result<Option<Vec<f32>>, String> {
        if self.finished {
            return Ok(None);
        }

        let mut output = frame::Audio::new(
            ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
            0,
            ffmpeg::ChannelLayout::STEREO,
        );

        output.set_rate(self.output_rate);

        // let delay = self
        //     .resampler
        //     .flush(&mut output)
        //     .map_err(|e| e.to_string())?;
        //
        let input = self.resampler.input();
        eprintln!(
            "Resampler input: format={:?}, rate={}, channels={}",
            input.format,
            input.rate,
            input.channel_layout.channels()
        );

        let output_def = self.resampler.output();
        eprintln!(
            "Resampler output: format={:?}, rate={}, channels={}",
            output_def.format,
            output_def.rate,
            output_def.channel_layout.channels()
        );
        let delay = match self.resampler.flush(&mut output) {
            Ok(delay) => delay,
            Err(e) => {
                eprintln!(
                    "Resampler flush failed: {e:?}; \
                     output format={:?}, rate={}, channels={}",
                    output.format(),
                    output.rate(),
                    output.channels()
                );
                return Err(e.to_string());
            }
        };

        if delay.is_none() || output.samples() == 0 {
            self.finished = true;
            return Ok(None);
        }

        let bytes = output.data(0);
        let mut pcm = Vec::with_capacity(output.samples() * CHANNELS);

        for sample in bytes.chunks_exact(4).take(output.samples() * CHANNELS) {
            pcm.push(f32::from_ne_bytes([
                sample[0], sample[1], sample[2], sample[3],
            ]));
        }

        self.output_cursor += (pcm.len() / CHANNELS) as u64;

        Ok(Some(pcm))
    }
}
