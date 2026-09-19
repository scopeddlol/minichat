use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

#[derive(Clone)]
pub struct AudioFeed {
    stopped: Arc<AtomicBool>,
    bytes: Arc<Mutex<VecDeque<u8>>>,
    error: Arc<Mutex<Option<String>>>,
}
impl Drop for AudioFeed {
    fn drop(&mut self) {
        self.stop();
    }
}
impl AudioFeed {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
    }
    pub fn read(&self) -> Result<Vec<u8>, String> {
        if let Some(error) = self.error.lock().map_err(|e| e.to_string())?.as_ref() {
            return Err(error.clone());
        }
        Ok(self
            .bytes
            .lock()
            .map_err(|e| e.to_string())?
            .drain(..)
            .collect())
    }
}

pub fn start(pid: u32, include: bool) -> Result<AudioFeed, String> {
    let feed = AudioFeed {
        stopped: Arc::new(AtomicBool::new(false)),
        bytes: Arc::new(Mutex::new(VecDeque::new())),
        error: Arc::new(Mutex::new(None)),
    };
    let worker = feed.clone();
    let (ready, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = run(&worker, pid, include, ready);
        if let Err(error) = result {
            if let Ok(mut slot) = worker.error.lock() {
                *slot = Some(error.to_string());
            }
        }
    });
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(feed),
        _ => {
            feed.stop();
            Err("System audio is unavailable. Try sharing without audio (process audio capture requires a recent Windows version).".into())
        }
    }
}

fn run(
    feed: &AudioFeed,
    pid: u32,
    include: bool,
    ready: std::sync::mpsc::SyncSender<Result<(), String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};
    wasapi::initialize_mta().ok()?;
    let mut client = AudioClient::new_application_loopback_client(pid, include)?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, 48000, 2, None);
    client.initialize_client(
        &format,
        &Direction::Capture,
        &StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: 200_000,
        },
    )?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    let mut buffer = VecDeque::new();
    client.start_stream()?;
    let _ = ready.send(Ok(()));
    while !feed.stopped.load(Ordering::Acquire) {
        // A silent source may not signal; timeouts still let cancellation finish.
        if event.wait_for_event(100).is_err() {
            continue;
        }
        capture.read_from_device_to_deque(&mut buffer)?;
        if let Ok(mut bytes) = feed.bytes.lock() {
            bytes.append(&mut buffer);
            // At most half a second of interleaved float stereo. Never accumulate
            // unbounded audio when the instance stops consuming IPC responses.
            let excess = bytes.len().saturating_sub(48000 * 8 / 2);
            bytes.drain(..excess - excess % 8);
        }
    }
    client.stop_stream()?;
    Ok(())
}
