use cpal::{
    SampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
};

const CHUNK_SAMPLES: usize = 4096;
const BUFFER_CHUNKS: usize = 16;

pub struct RadioPlayer {
    stream: Mutex<Option<Stream>>,
    child: Arc<Mutex<Option<Child>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    running: Arc<AtomicBool>,
}

impl RadioPlayer {
    pub fn new() -> Self {
        Self {
            stream: Mutex::new(None),
            child: Arc::new(Mutex::new(None)),
            worker: Mutex::new(None),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_playing(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn stop(&self) -> Result<(), String> {
        self.running.store(false, Ordering::Relaxed);
        *self.stream.lock().map_err(|e| e.to_string())? = None;

        {
            let mut child = self.child.lock().map_err(|e| e.to_string())?;

            if let Some(process) = child.as_mut() {
                let _ = process.kill();
            }
        }

        if let Some(worker) = self.worker.lock().map_err(|e| e.to_string())?.take() {
            let _ = worker.join();
        }

        if let Some(mut process) = self.child.lock().map_err(|e| e.to_string())?.take() {
            let _ = process.wait();
        }

        Ok(())
    }

    pub fn start(&self, url: &str) -> Result<(), String> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err("Radio URL must use HTTP or HTTPS".into());
        }

        self.stop()?;

        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .ok_or("No audio output device found")?;

        let supported = device.default_output_config().map_err(|e| e.to_string())?;

        let format = supported.sample_format();
        let config: StreamConfig = supported.into();

        if config.sample_rate == 0 || config.channels == 0 {
            return Err("Invalid output configuration".into());
        }

        let rate = config.sample_rate;
        let mut process = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-i",
                url,
                "-vn",
                "-f",
                "f32le",
                "-acodec",
                "pcm_f32le",
                "-ac",
                "2",
                "-ar",
                &rate.to_string(),
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Could not start FFmpeg: {e}"))?;

        let mut stdout = process
            .stdout
            .take()
            .ok_or("Could not open FFmpeg audio output")?;

        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(BUFFER_CHUNKS);

        let running = Arc::clone(&self.running);
        running.store(true, Ordering::Relaxed);

        let worker = thread::spawn(move || {
            let mut bytes = vec![0u8; CHUNK_SAMPLES * 4];

            while running.load(Ordering::Relaxed) {
                if stdout.read_exact(&mut bytes).is_err() {
                    break;
                }

                let samples: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();

                if tx.send(samples).is_err() {
                    break;
                }
            }

            running.store(false, Ordering::Relaxed);
        });

        *self.child.lock().map_err(|e| e.to_string())? = Some(process);
        *self.worker.lock().map_err(|e| e.to_string())? = Some(worker);

        let channels = config.channels as usize;
        let stream_result = match format {
            SampleFormat::F32 => {
                let mut reader = ChunkReader::new(rx);

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [f32], _| {
                        render(data, channels, &mut reader, |s| s.clamp(-1.0, 1.0));
                    },
                    |e| eprintln!("Radio CPAL error: {e}"),
                    None,
                )
            }

            SampleFormat::I16 => {
                let mut reader = ChunkReader::new(rx);

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [i16], _| {
                        render(data, channels, &mut reader, |s| {
                            (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                        });
                    },
                    |e| eprintln!("Radio CPAL error: {e}"),
                    None,
                )
            }

            SampleFormat::U16 => {
                let mut reader = ChunkReader::new(rx);

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [u16], _| {
                        render(data, channels, &mut reader, |s| {
                            ((s.clamp(-1.0, 1.0) + 1.0) * 0.5 * u16::MAX as f32) as u16
                        });
                    },
                    |e| eprintln!("Radio CPAL error: {e}"),
                    None,
                )
            }

            other => {
                self.stop()?;
                return Err(format!("Unsupported audio format: {other:?}"));
            }
        };

        let stream = match stream_result {
            Ok(stream) => stream,
            Err(error) => {
                self.stop()?;
                return Err(error.to_string());
            }
        };

        if let Err(error) = stream.play() {
            self.stop()?;
            return Err(error.to_string());
        }

        *self.stream.lock().map_err(|e| e.to_string())? = Some(stream);

        Ok(())
    }
}

struct ChunkReader {
    rx: Receiver<Vec<f32>>,
    chunk: Vec<f32>,
    cursor: usize,
}

impl ChunkReader {
    fn new(rx: Receiver<Vec<f32>>) -> Self {
        Self {
            rx,
            chunk: Vec::new(),
            cursor: 0,
        }
    }

    fn next_frame(&mut self) -> (f32, f32) {
        if self.cursor + 1 >= self.chunk.len() {
            match self.rx.try_recv() {
                Ok(chunk) => {
                    self.chunk = chunk;
                    self.cursor = 0;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                    return (0.0, 0.0);
                }
            }
        }

        if self.cursor + 1 >= self.chunk.len() {
            return (0.0, 0.0);
        }

        let left = self.chunk[self.cursor];
        let right = self.chunk[self.cursor + 1];

        self.cursor += 2;

        (left, right)
    }
}

fn render<T>(
    data: &mut [T],
    channels: usize,
    reader: &mut ChunkReader,
    mut convert: impl FnMut(f32) -> T,
) {
    for frame in data.chunks_mut(channels) {
        let (left, right) = reader.next_frame();

        for (channel, output) in frame.iter_mut().enumerate() {
            let sample = if channels == 1 {
                (left + right) * 0.5
            } else if channel == 0 {
                left
            } else if channel == 1 {
                right
            } else {
                0.0
            };

            *output = convert(sample * 0.65);
        }
    }
}
