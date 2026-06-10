use serde_json::{json, Value};
use std::io::Read;
use std::sync::{mpsc, Arc, LazyLock, Mutex};
use std::thread;
use std::time::Duration;
use tiny_http::{Response, ResponseBox, StatusCode};

pub(crate) static EVENT_SUBSCRIBERS: LazyLock<Mutex<Vec<mpsc::Sender<Vec<u8>>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub(crate) const SSE_FLUSH_PADDING_BYTES: usize = 8193;

pub(crate) struct EventStreamReader {
    receiver: mpsc::Receiver<Vec<u8>>,
    buffer: Vec<u8>,
    offset: usize,
}

impl EventStreamReader {
    fn new(receiver: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            receiver,
            buffer: Vec::new(),
            offset: 0,
        }
    }
}

impl Read for EventStreamReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        while self.offset >= self.buffer.len() {
            match self.receiver.recv() {
                Ok(next) => {
                    self.buffer = next;
                    self.offset = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let count = out.len().min(self.buffer.len() - self.offset);
        out[..count].copy_from_slice(&self.buffer[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

pub(crate) fn sse_frame(event: &str, data: Value) -> Vec<u8> {
    let payload = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    let mut frame = format!("event: {event}\ndata: {payload}\n\n").into_bytes();
    if frame.len() < SSE_FLUSH_PADDING_BYTES {
        frame.extend_from_slice(b":");
        frame.extend(std::iter::repeat(b' ').take(SSE_FLUSH_PADDING_BYTES - frame.len()));
        frame.extend_from_slice(b"\n\n");
    }
    frame
}

pub fn publish_event(event: &str, data: Value) {
    let frame = sse_frame(event, data);
    if let Ok(mut subscribers) = EVENT_SUBSCRIBERS.lock() {
        subscribers.retain(|subscriber| subscriber.send(frame.clone()).is_ok());
    }
}

pub(crate) fn install_core_event_publishers() {
    cockpit_core::modules::codex_local_access::set_local_access_event_publisher(Some(Arc::new(
        |event, payload| publish_event(event, payload),
    )));
    cockpit_core::modules::codex_account::set_codex_batch_import_event_publisher(Some(Arc::new(
        |event, payload| publish_event(event, payload),
    )));
}

pub(crate) fn subscribe_events() -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    let _ = sender.send(sse_frame(
        "service.ready",
        json!({
            "emittedAt": chrono::Utc::now().timestamp_millis(),
            "version": env!("CARGO_PKG_VERSION")
        }),
    ));
    if let Ok(mut subscribers) = EVENT_SUBSCRIBERS.lock() {
        subscribers.push(sender.clone());
    }
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(15));
        if sender
            .send(sse_frame(
                "service.heartbeat",
                json!({ "emittedAt": chrono::Utc::now().timestamp_millis() }),
            ))
            .is_err()
        {
            break;
        }
    });
    receiver
}

pub(crate) fn events_response() -> ResponseBox {
    let headers = [
        ("content-type", "text/event-stream; charset=utf-8"),
        ("cache-control", "no-cache"),
        ("x-accel-buffering", "no"),
    ]
    .into_iter()
    .filter_map(|(name, value)| crate::make_header(name, value))
    .collect();
    Response::new(
        StatusCode(200),
        headers,
        EventStreamReader::new(subscribe_events()),
        None,
        None,
    )
    .with_chunked_threshold(0)
    .boxed()
}
