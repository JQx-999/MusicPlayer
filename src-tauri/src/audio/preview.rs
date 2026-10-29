use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use ffmpeg::frame::Audio as AudioFrame;
use ffmpeg::software::resampling::Context as Resampler;
use ffmpeg_next as ffmpeg;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

#[allow(dead_code)]
pub(crate) fn decode_preview(path: &str, output_rate: u32) -> Result<Vec<f32>, String> {
    if output_rate == 0 {
        return Err("Invalid output sample rate".into());
    }

    ffmpeg::init().map_err(|e| e.to_string())?;

    let mut input = ffmpeg::format::input(path).map_err(|e| e.to_string())?;

    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or("No audio stream found")?;

    let stream_index = stream.index();

    let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| e.to_string())?;

    let mut decoder = context.decoder().audio().map_err(|e| e.to_string())?;

    let input_rate = decoder.rate();

    let mut resampler = ffmpeg::software::resampling::Context::get(
        decoder.format(),
        decoder.channel_layout(),
        input_rate,
        ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
        ffmpeg::channel_layout::ChannelLayout::STEREO,
        output_rate,
    )
    .map_err(|e| e.to_string())?;

    let mut pcm = Vec::<f32>::new();

    let mut decoded = ffmpeg::frame::Audio::empty();

    let mut append_frame = |decoded: &ffmpeg::frame::Audio| -> Result<(), String> {
        let capacity = ((decoded.samples() as u64 * output_rate as u64 + input_rate as u64 - 1)
            / input_rate as u64) as usize
            + 256;

        let mut converted = ffmpeg::frame::Audio::new(
            ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
            capacity,
            ffmpeg::channel_layout::ChannelLayout::STEREO,
        );

        converted.set_rate(output_rate);

        resampler
            .run(decoded, &mut converted)
            .map_err(|e| e.to_string())?;

        if converted.samples() > 0 {
            let valid_len = converted.samples() * 2;
            let data = converted.plane::<f32>(0);

            pcm.extend_from_slice(&data[..valid_len]);
        }

        Ok(())
    };

    for (packet_stream, packet) in input.packets() {
        if packet_stream.index() != stream_index {
            continue;
        }

        decoder.send_packet(&packet).map_err(|e| e.to_string())?;

        while decoder.receive_frame(&mut decoded).is_ok() {
            append_frame(&decoded)?;
        }
    }

    decoder.send_eof().map_err(|e| e.to_string())?;

    while decoder.receive_frame(&mut decoded).is_ok() {
        append_frame(&decoded)?;
    }

    drop(append_frame);

    loop {
        let mut tail = ffmpeg::frame::Audio::new(
            ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
            4096,
            ffmpeg::channel_layout::ChannelLayout::STEREO,
        );

        tail.set_rate(output_rate);

        resampler.flush(&mut tail).map_err(|e| e.to_string())?;

        if tail.samples() == 0 {
            break;
        }

        let valid_len = tail.samples() * 2;
        let data = tail.plane::<f32>(0);

        pcm.extend_from_slice(&data[..valid_len]);
    }

    if pcm.is_empty() {
        return Err("FFmpeg decoded no audio samples".into());
    }

    Ok(pcm)
}

pub fn play_preview(path: &str) -> Result<String, String> {
    const PREVIEW_SECS: f32 = 5.0;

    let handle = start_preview(path, Some(PREVIEW_SECS))?;

    let deadline = Instant::now() + Duration::from_secs_f32(PREVIEW_SECS + 5.0);
    while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }

    drop(handle);
    Ok("Native music preview completed!".into())
}

const RING_SECONDS: usize = 2;
const VOLUME: f32 = 0.65;

#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    paused: AtomicBool,
    decoder_done: AtomicBool,
    finished: AtomicBool,
}

pub struct PreviewHandle {
    shared: Arc<Shared>,
    audio_thread: Option<JoinHandle<()>>,
}

impl PreviewHandle {
    #[allow(dead_code)]
    pub fn set_paused(&self, paused: bool) {
        self.shared.paused.store(paused, SeqCst);
    }
    pub fn is_finished(&self) -> bool {
        self.shared.finished.load(SeqCst)
    }
    pub fn stop(&mut self) {
        self.shared.stop.store(true, SeqCst);
        if let Some(t) = self.audio_thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for PreviewHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start_preview(path: &str, max_seconds: Option<f32>) -> Result<PreviewHandle, String> {
    let shared = Arc::new(Shared::default());

    let (dev_tx, dev_rx) = mpsc::sync_channel::<Result<(u32, HeapProd<f32>), String>>(1);
    let audio_shared = shared.clone();
    let audio_thread = thread::Builder::new()
        .name("audio-output".into())
        .spawn(move || audio_thread_main(audio_shared, dev_tx))
        .map_err(|e| e.to_string())?;

    let handle = PreviewHandle {
        shared: shared.clone(),
        audio_thread: Some(audio_thread),
    };

    let (rate, prod) = dev_rx
        .recv()
        .map_err(|_| "Audio thread exited unexpectedly".to_string())??;

    let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);
    let dec_shared = shared.clone();
    let path = path.to_owned();
    let max_samples = max_seconds.map(|s| (s.max(0.0) * rate as f32) as usize * 2);

    thread::Builder::new()
        .name("audio-decoder".into())
        .spawn(move || {
            match DecoderState::open(&path, rate) {
                Ok(mut dec) => {
                    let _ = ready_tx.send(Ok(()));
                    let mut sink = Sink {
                        prod,
                        shared: &dec_shared,
                        remaining: max_samples,
                    };
                    if let Err(e) = dec.run(&mut sink) {
                        eprintln!("Decoder error: {e}");
                    }
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
            dec_shared.decoder_done.store(true, SeqCst);
        })
        .map_err(|e| e.to_string())?;

    ready_rx
        .recv()
        .map_err(|_| "Decoder thread exited unexpectedly".to_string())??;

    Ok(handle)
}

fn audio_thread_main(
    shared: Arc<Shared>,
    dev_tx: mpsc::SyncSender<Result<(u32, HeapProd<f32>), String>>,
) {
    let setup = || -> Result<(Stream, u32, HeapProd<f32>), String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No audio output device found")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();

        let rate = config.sample_rate;
        let channels = config.channels as usize;
        if channels == 0 || rate == 0 {
            return Err("Audio device reported invalid configuration".into());
        }

        let rb = HeapRb::<f32>::new(rate as usize * 2 * RING_SECONDS);
        let (prod, cons) = rb.split();
        let prebuffer = rate as usize;

        let sh = shared.clone();
        let stream = match sample_format {
            SampleFormat::F32 => {
                build_stream::<f32>(&device, config, channels, cons, sh, prebuffer)
            }
            SampleFormat::I16 => {
                build_stream::<i16>(&device, config, channels, cons, sh, prebuffer)
            }
            SampleFormat::U16 => {
                build_stream::<u16>(&device, config, channels, cons, sh, prebuffer)
            }
            SampleFormat::I32 => {
                build_stream::<i32>(&device, config, channels, cons, sh, prebuffer)
            }
            other => return Err(format!("Unsupported output sample format: {other:?}")),
        }
        .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;
        Ok((stream, rate, prod))
    };

    match setup() {
        Ok((stream, rate, prod)) => {
            let _ = dev_tx.send(Ok((rate, prod)));
            while !shared.stop.load(SeqCst) && !shared.finished.load(SeqCst) {
                thread::sleep(Duration::from_millis(20));
            }
            if shared.finished.load(SeqCst) && !shared.stop.load(SeqCst) {
                thread::sleep(Duration::from_millis(300));
            }
            drop(stream);
        }
        Err(e) => {
            let _ = dev_tx.send(Err(e));
        }
    }
    shared.finished.store(true, SeqCst);
}
fn build_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    channels: usize,
    mut cons: HeapCons<f32>,
    shared: Arc<Shared>,
    prebuffer: usize,
) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let mut started = false;

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let silence = T::from_sample(0.0f32);

                if shared.paused.load(SeqCst) {
                    data.fill(silence);
                    return;
                }

                let done = shared.decoder_done.load(SeqCst);

                if !started {
                    if cons.occupied_len() >= prebuffer || done {
                        started = true;
                    } else {
                        data.fill(silence);
                        return;
                    }
                }

                for frame in data.chunks_mut(channels) {
                    let (l, r) = if cons.occupied_len() >= 2 {
                        (cons.try_pop().unwrap_or(0.0), cons.try_pop().unwrap_or(0.0))
                    } else {
                        if done {
                            shared.finished.store(true, SeqCst);
                        }
                        (0.0, 0.0)
                    };

                    for (i, out) in frame.iter_mut().enumerate() {
                        let s = if channels == 1 {
                            (l + r) * 0.5
                        } else {
                            match i {
                                0 => l,
                                1 => r,
                                _ => 0.0,
                            }
                        };
                        *out = T::from_sample((s * VOLUME).clamp(-1.0, 1.0));
                    }
                }
            },
            |e| eprintln!("CPAL error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

struct Sink<'a> {
    prod: HeapProd<f32>,
    shared: &'a Shared,
    remaining: Option<usize>,
}

impl Sink<'_> {
    fn write(&mut self, mut data: &[f32]) -> bool {
        let mut limit_hit = false;
        if let Some(rem) = self.remaining {
            if data.len() >= rem {
                data = &data[..rem];
                limit_hit = true;
            }
            self.remaining = Some(rem - data.len());
        }
        while !data.is_empty() {
            if self.shared.stop.load(SeqCst) {
                return false;
            }
            let n = self.prod.push_slice(data);
            data = &data[n..];
            if n == 0 {
                thread::sleep(Duration::from_millis(5));
            }
        }
        !limit_hit
    }
}

struct DecoderState {
    input: ffmpeg::format::context::Input,
    decoder: ffmpeg::decoder::Audio,
    resampler: Resampler,
    stream_index: usize,
    in_rate: u32,
    out_rate: u32,
}

fn packed_stereo(samples: usize) -> AudioFrame {
    AudioFrame::new(
        ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
        samples,
        ffmpeg::channel_layout::ChannelLayout::STEREO,
    )
}

fn frame_to_f32(frame: &AudioFrame, out: &mut Vec<f32>) {
    let bytes = frame.data(0);
    let len = (frame.samples() * 2 * 4).min(bytes.len());
    out.clear();
    out.extend(
        bytes[..len]
            .chunks_exact(4)
            .map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]])),
    );
}

impl DecoderState {
    fn open(path: &str, out_rate: u32) -> Result<Self, String> {
        ffmpeg::init().map_err(|e| e.to_string())?;
        let input = ffmpeg::format::input(path).map_err(|e| e.to_string())?;

        let (stream_index, decoder) = {
            let stream = input
                .streams()
                .best(ffmpeg::media::Type::Audio)
                .ok_or("No audio stream found")?;
            let ctx = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
                .map_err(|e| e.to_string())?;
            (
                stream.index(),
                ctx.decoder().audio().map_err(|e| e.to_string())?,
            )
        };

        let in_rate = decoder.rate();
        let resampler = Resampler::get(
            decoder.format(),
            decoder.channel_layout(),
            in_rate,
            ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
            ffmpeg::channel_layout::ChannelLayout::STEREO,
            out_rate,
        )
        .map_err(|e| e.to_string())?;

        Ok(Self {
            input,
            decoder,
            resampler,
            stream_index,
            in_rate,
            out_rate,
        })
    }

    fn convert_and_push(
        resampler: &mut Resampler,
        decoded: &AudioFrame,
        in_rate: u32,
        out_rate: u32,
        scratch: &mut Vec<f32>,
        sink: &mut Sink,
    ) -> Result<bool, String> {
        let capacity = ((decoded.samples() as u64 * out_rate as u64 + in_rate as u64 - 1)
            / in_rate as u64) as usize
            + 256;
        let mut converted = packed_stereo(capacity);
        converted.set_rate(out_rate);

        resampler
            .run(decoded, &mut converted)
            .map_err(|e| e.to_string())?;

        if converted.samples() == 0 {
            return Ok(true);
        }
        frame_to_f32(&converted, scratch);
        Ok(sink.write(scratch))
    }

    fn run(&mut self, sink: &mut Sink) -> Result<(), String> {
        let Self {
            input,
            decoder,
            resampler,
            stream_index,
            in_rate,
            out_rate,
        } = self;
        let (in_rate, out_rate, stream_index) = (*in_rate, *out_rate, *stream_index);

        let mut decoded = AudioFrame::empty();
        let mut scratch: Vec<f32> = Vec::new();

        for (stream, packet) in input.packets() {
            if sink.shared.stop.load(SeqCst) {
                return Ok(());
            }
            if stream.index() != stream_index {
                continue;
            }
            if decoder.send_packet(&packet).is_err() {
                continue;
            }
            while decoder.receive_frame(&mut decoded).is_ok() {
                if !Self::convert_and_push(
                    resampler,
                    &decoded,
                    in_rate,
                    out_rate,
                    &mut scratch,
                    sink,
                )? {
                    return Ok(());
                }
            }
        }

        let _ = decoder.send_eof();
        while decoder.receive_frame(&mut decoded).is_ok() {
            if !Self::convert_and_push(resampler, &decoded, in_rate, out_rate, &mut scratch, sink)?
            {
                return Ok(());
            }
        }

        loop {
            let mut tail = packed_stereo(4096);
            tail.set_rate(out_rate);
            resampler.flush(&mut tail).map_err(|e| e.to_string())?;
            if tail.samples() == 0 {
                break;
            }
            frame_to_f32(&tail, &mut scratch);
            if !sink.write(&scratch) {
                break;
            }
        }
        Ok(())
    }
}
