//! Shared harness for the network-policy and multi-agent state tests.
//!
//! Everything here is loopback-only and credential-free: no test in these files may reach a real
//! Exa endpoint, and none of them may read or write the developer's real `$HOME`.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A throwaway directory unique to this process and call.
pub fn scratch_dir(label: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "exa-agent-{label}-{}-{stamp}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    exa_agent_cli::fsutil::create_dir_all_private(&dir).expect("create private scratch dir");
    dir
}

/// A CLI invocation with every managed-state path redirected into `home`, and every inherited
/// credential/output/profile override stripped.
pub fn isolated_command(home: &std::path::Path) -> Command {
    isolated_program(home, env!("CARGO_BIN_EXE_exa-agent"))
}

pub fn isolated_program(home: &std::path::Path, program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .env_remove("EXA_API_KEY")
        .env_remove("EXA_SERVICE_KEY")
        .env_remove("EXA_PROFILE")
        .env_remove("EXA_OUTPUT")
        .env_remove("EXA_ADMIN_BASE_URL")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_STATE_HOME")
        .env("EXA_AGENT_NO_NETWORK", "1")
        .env("HOME", home)
        .env("EXA_AGENT_CONFIG", home.join("config.toml"))
        .env("EXA_AGENT_CREDENTIALS", home.join("credentials.json"))
        .env("EXA_AGENT_PRESETS", home.join("presets.toml"))
        .env("EXA_AGENT_LOCAL_PRESETS", home.join("local-presets.toml"))
        .env("EXA_AGENT_STATE", home.join("state"))
        .env("EXA_AGENT_PENDING_RUNS", home.join("pending-runs.jsonl"));
    command
}

/// The same isolation, with the network guard lifted so the command may talk to a local
/// loopback fixture. Never point one of these at a non-loopback URL.
pub fn loopback_command(home: &std::path::Path) -> Command {
    let mut command = isolated_command(home);
    command.env_remove("EXA_AGENT_NO_NETWORK");
    command
}

/// What one fixture request should produce.
pub struct Reply {
    /// Held open before the response is written, to make overlap observable.
    pub delay: Duration,
    /// Complete raw HTTP response bytes, status line included.
    pub bytes: Vec<u8>,
}

impl Reply {
    pub fn json(body: &str) -> Self {
        Self::json_after(Duration::ZERO, body)
    }

    pub fn json_after(delay: Duration, body: &str) -> Self {
        Self {
            delay,
            bytes: http_response(200, "application/json", None, body.as_bytes()),
        }
    }

    pub fn status(status: u16, body: &str) -> Self {
        Self {
            delay: Duration::ZERO,
            bytes: http_response(status, "application/json", None, body.as_bytes()),
        }
    }

    pub fn raw(delay: Duration, bytes: Vec<u8>) -> Self {
        Self { delay, bytes }
    }
}

pub fn http_response(
    status: u16,
    content_type: &str,
    content_encoding: Option<&str>,
    body: &[u8],
) -> Vec<u8> {
    let mut head = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(encoding) = content_encoding {
        head.push_str(&format!("Content-Encoding: {encoding}\r\n"));
    }
    head.push_str("Connection: close\r\n\r\n");
    let mut out = head.into_bytes();
    out.extend_from_slice(body);
    out
}

#[derive(Default, Debug)]
pub struct ServerStats {
    /// Full request text (head + body) in the order requests were fully read.
    pub requests: Vec<String>,
    /// Highest number of requests being served at the same instant.
    pub max_concurrent: usize,
}

type Responder = Arc<dyn Fn(usize, &str) -> Reply + Send + Sync>;

/// A loopback HTTP fixture that records concurrency and request heads.
pub struct TestServer {
    pub base_url: String,
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    stats: Arc<Mutex<ServerStats>>,
    active: Arc<AtomicUsize>,
    accept: Option<thread::JoinHandle<()>>,
}

impl TestServer {
    /// Answer request `i` with `replies[i]`, or with the last reply once they run out.
    ///
    /// Arrival order is only meaningful for a serial client. Concurrent clients must use
    /// [`TestServer::start_routed`] and key the reply off the request itself.
    pub fn start(replies: Vec<Reply>) -> Self {
        let replies = Arc::new(replies);
        Self::start_routed(move |index, _| {
            let reply = replies.get(index).or_else(|| replies.last());
            match reply {
                Some(reply) => Reply {
                    delay: reply.delay,
                    bytes: reply.bytes.clone(),
                },
                None => Reply::status(500, r#"{"error":{"message":"no fixture reply"}}"#),
            }
        })
    }

    /// Answer each request from its own bytes, so a concurrent client gets a deterministic
    /// reply no matter which connection the kernel accepts first.
    pub fn start_routed<F>(responder: F) -> Self
    where
        F: Fn(usize, &str) -> Reply + Send + Sync + 'static,
    {
        let responder: Responder = Arc::new(responder);
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
        let addr = listener.local_addr().expect("fixture address");
        let shutdown = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(Mutex::new(ServerStats::default()));
        let active = Arc::new(AtomicUsize::new(0));

        let accept = {
            let shutdown = Arc::clone(&shutdown);
            let stats = Arc::clone(&stats);
            let active = Arc::clone(&active);
            thread::spawn(move || {
                let mut workers = Vec::new();
                for (index, stream) in listener.incoming().enumerate() {
                    if shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream else { break };
                    let responder = Arc::clone(&responder);
                    let stats = Arc::clone(&stats);
                    let active = Arc::clone(&active);
                    workers.push(thread::spawn(move || {
                        serve_one(stream, index, responder.as_ref(), &stats, &active)
                    }));
                }
                for worker in workers {
                    let _ = worker.join();
                }
            })
        };

        Self {
            base_url: format!("http://{addr}"),
            addr,
            shutdown,
            stats,
            active,
            accept: Some(accept),
        }
    }

    /// Stop accepting and return what the fixture observed.
    pub fn finish(mut self) -> ServerStats {
        self.shutdown.store(true, Ordering::SeqCst);
        // Unblock `incoming()` with one throwaway connection.
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.accept.take() {
            let _ = handle.join();
        }
        std::mem::take(&mut *self.stats.lock().expect("fixture stats"))
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.accept.take() {
            let _ = handle.join();
        }
    }
}

fn serve_one(
    mut stream: TcpStream,
    index: usize,
    responder: &(dyn Fn(usize, &str) -> Reply + Send + Sync),
    stats: &Mutex<ServerStats>,
    active: &AtomicUsize,
) {
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
    {
        let mut stats = stats.lock().expect("fixture stats");
        stats.requests.push(request.clone());
        stats.max_concurrent = stats.max_concurrent.max(now);
    }
    let reply = responder(index, &request);
    if !reply.delay.is_zero() {
        thread::sleep(reply.delay);
    }
    let _ = stream.write_all(&reply.bytes);
    let _ = stream.flush();
    active.fetch_sub(1, Ordering::SeqCst);
    let _ = stream.shutdown(std::net::Shutdown::Write);
    // Drain so the client is never reset while it is still reading the response.
    let _ = stream.read_to_end(&mut Vec::new());
}

/// Read headers plus any declared body; returns the whole request as text.
fn read_request(stream: &mut TcpStream) -> Option<String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("fixture read timeout");
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    let started = Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(10) {
            return None;
        }
        let header_end = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4);
        if let Some(header_end) = header_end {
            let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
            let content_length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            let _ = &head;
            if buf.len() >= header_end + content_length {
                return Some(String::from_utf8_lossy(&buf).into_owned());
            }
        }
        match stream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => return None,
        }
    }
}

/// `error.code` from a CLI error envelope on stderr.
pub fn stderr_json(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stderr).unwrap_or_else(|err| {
        panic!(
            "stderr was not JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

pub fn stdout_lines(output: &std::process::Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|err| panic!("stdout line not JSON: {err}\n{line}"))
        })
        .collect()
}
