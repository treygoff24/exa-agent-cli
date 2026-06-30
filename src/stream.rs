//! Blocking SSE helpers: incremental framing plus the SIGINT flag polled by transport.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::time::Duration;

use crate::error::{CliError, Diag};
use crate::transport::SseFrame;

pub const SSE_READ_TIMEOUT: Duration = Duration::from_millis(250);

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static INSTALL_SIGINT: Once = Once::new();

pub fn install_sigint_handler() -> Result<(), CliError> {
    let mut failed = None;
    INSTALL_SIGINT.call_once(|| {
        failed = ctrlc::set_handler(|| {
            INTERRUPTED.store(true, Ordering::SeqCst);
        })
        .err();
    });
    failed.map_or(Ok(()), |err| {
        Err(CliError::Config(Diag::new(
            "config_error",
            format!("failed to install SIGINT handler: {err}"),
        )))
    })
}

pub fn reset_interrupt() {
    INTERRUPTED.store(false, Ordering::SeqCst);
}

pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

pub fn interrupted_stream_error(last_event_id: Option<&str>) -> CliError {
    let mut diag = Diag::new("interrupted", "stream interrupted");
    diag.retryable = false;
    if let Some(id) = last_event_id {
        diag = diag.with_details(serde_json::json!({ "lastEventId": id }));
    }
    CliError::Interrupted(diag)
}

pub fn is_poll_timeout(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

#[derive(Debug, Default)]
pub struct SseDecoder {
    line: Vec<u8>,
    id: Option<String>,
    data: Vec<String>,
    last_event_id: Option<String>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseFrame> {
        let mut frames = Vec::new();
        for &byte in bytes {
            if byte == b'\n' {
                if self.line.last() == Some(&b'\r') {
                    self.line.pop();
                }
                let line = String::from_utf8_lossy(&self.line).into_owned();
                self.line.clear();
                self.process_line(&line, &mut frames);
            } else {
                self.line.push(byte);
            }
        }
        frames
    }

    pub fn finish(&mut self) -> Vec<SseFrame> {
        let mut frames = Vec::new();
        if !self.line.is_empty() {
            if self.line.last() == Some(&b'\r') {
                self.line.pop();
            }
            let line = String::from_utf8_lossy(&self.line).into_owned();
            self.line.clear();
            self.process_line(&line, &mut frames);
        }
        self.flush_frame(&mut frames);
        frames
    }

    pub fn last_event_id(&self) -> Option<&str> {
        self.last_event_id.as_deref()
    }

    fn process_line(&mut self, line: &str, frames: &mut Vec<SseFrame>) {
        if line.is_empty() {
            self.flush_frame(frames);
        } else if line.starts_with(':') {
            // Comment/heartbeat.
        } else if let Some(rest) = line.strip_prefix("id:") {
            let id = rest.trim_start().to_string();
            self.id = Some(id);
        } else if let Some(rest) = line.strip_prefix("data:") {
            self.data.push(rest.trim_start().to_string());
        }
    }

    fn flush_frame(&mut self, frames: &mut Vec<SseFrame>) {
        if self.id.is_some() || !self.data.is_empty() {
            if let Some(id) = &self.id {
                self.last_event_id = Some(id.clone());
            }
            frames.push(SseFrame {
                id: self.id.take(),
                data: std::mem::take(&mut self.data),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_decoder_frames_split_chunks_and_tracks_last_id() {
        let mut decoder = SseDecoder::new();
        assert!(decoder.push(b"id: evt-1\ndata: {\"a\"").is_empty());
        let frames = decoder.push(b":1}\n\nid: evt-2\ndata: [DONE]\n\n");

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].id.as_deref(), Some("evt-1"));
        assert_eq!(frames[0].data, vec![r#"{"a":1}"#]);
        assert_eq!(frames[1].id.as_deref(), Some("evt-2"));
        assert_eq!(decoder.last_event_id(), Some("evt-2"));
    }

    #[test]
    fn stream_decoder_does_not_advance_last_id_until_frame_completes() {
        let mut decoder = SseDecoder::new();
        assert!(decoder.push(b"id: evt-1\n").is_empty());
        assert_eq!(decoder.last_event_id(), None);
        assert!(decoder.push(b"data: {\"ok\":true}\n").is_empty());
        assert_eq!(decoder.last_event_id(), None);

        let frames = decoder.push(b"\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].id.as_deref(), Some("evt-1"));
        assert_eq!(decoder.last_event_id(), Some("evt-1"));
    }

    #[test]
    fn interrupted_error_carries_last_event_id_when_available() {
        let err = interrupted_stream_error(Some("evt-9"));
        assert_eq!(err.category(), 12);
        assert_eq!(err.diag().code, "interrupted");
        assert_eq!(err.diag().details.as_ref().unwrap()["lastEventId"], "evt-9");
    }
}
