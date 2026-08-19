// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-17

//! Shared test helpers. Each task adds the ones it needs, and the tasks that
//! follow reuse them; the module starts empty on purpose.
//!
//! Filesystem fixtures are built BY HAND rather than via a crate such as
//! `tempfile`: this package takes zero new dependencies (see the header of
//! `smoke/Cargo.toml`), and `#[cfg(test)]`-only code is exactly where
//! `unwrap`/`expect` are permitted, so a small hand-rolled helper costs
//! nothing extra in rigor.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// A directory under the OS temp root, removed on drop.
///
/// Owns the path so a test cannot outlive its own fixture directory by
/// accident: cleanup happens exactly once, when the last reference to the
/// `TempDir` goes out of scope.
pub struct TempDir(PathBuf);

impl TempDir {
    /// The directory's path, for handing to the code under test.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    /// Best-effort cleanup. A `Drop` impl cannot return `Result`, and a test
    /// process exiting with the OS temp directory holding a few stray bytes
    /// is not a failure worth propagating — the next run gets a fresh,
    /// uniquely-named directory regardless (see [`tempdir_with`]).
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Monotonic counter mixed into each generated directory name so that two
/// `tempdir_with` calls within the same nanosecond (plausible on a fast
/// machine, since `SystemTime` resolution is not guaranteed) still collide
/// on nothing.
static UNIQUE: AtomicU64 = AtomicU64::new(0);

/// Creates a fresh temporary directory and populates it with `files`: pairs
/// of a path relative to the directory root and the text content to write
/// there. Parent directories are created as needed, so `("src/a/b.rs", "..")`
/// is valid without `src/a` existing beforehand.
///
/// # Panics
///
/// Panics if the directory or any file cannot be created. Acceptable here:
/// this is `#[cfg(test)]`-only fixture setup, and a setup failure should stop
/// the test immediately and loudly rather than run against a partial tree.
pub fn tempdir_with(files: &[(&str, &str)]) -> TempDir {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is set to a time after the Unix epoch")
        .as_nanos();
    let unique = UNIQUE.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "magi-smoke-test-{}-{nanos}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create temp fixture directory");
    for (rel, content) in files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("failed to create parent directory");
        }
        std::fs::write(&path, content).expect("failed to write fixture file");
    }
    TempDir(dir)
}

/// Windows `ERROR_PRIVILEGE_NOT_HELD`. Creating a *symlink* there requires
/// Developer Mode or an elevated process, and the refusal arrives as a raw OS
/// code that `std` does not map to a named [`std::io::ErrorKind`] — so matching
/// on `PermissionDenied` alone silently misses it, which is how this check
/// failed the first time it was written.
const WINDOWS_PRIVILEGE_NOT_HELD: i32 = 1314;

/// Whether the OS refused the operation for lack of privilege, as opposed to
/// failing for a reason that would be a real defect.
fn is_privilege_refusal(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::PermissionDenied
        || e.raw_os_error() == Some(WINDOWS_PRIVILEGE_NOT_HELD)
}

/// Creates a file symlink at `link` pointing at `target`, and reports whether
/// it now exists.
///
/// # Why this returns a `bool` instead of panicking
///
/// Creating a symlink is a PRIVILEGED operation on Windows unless Developer
/// Mode is on, and some Unix filesystems refuse it too. That refusal says
/// nothing about the code under test, so the caller is told "could not create"
/// and decides for itself — rather than turning a fact about the machine into
/// a red test, which is how a gate stops being believed.
///
/// A junction is NOT a usable fallback here, and that was measured rather than
/// assumed: `symlink_metadata` reports `is_dir() == false` for one, so the
/// walker under test ignores it whether its guard is present or not.
///
/// # Panics
///
/// On any failure that is **not** a privilege refusal. Those are real defects
/// and must not be swallowed.
pub fn make_symlink(link: PathBuf, target: PathBuf) -> bool {
    #[cfg(unix)]
    let created = std::os::unix::fs::symlink(&target, &link);
    #[cfg(windows)]
    let created = std::os::windows::fs::symlink_file(&target, &link);

    match created {
        Ok(()) => true,
        Err(e) if is_privilege_refusal(&e) => false,
        Err(e) => panic!("failed to create symlink {link:?} -> {target:?}: {e}"),
    }
}

/// Creates a DIRECTORY link at `link` pointing at the directory `target`, and
/// reports whether it now exists.
///
/// # Why this is separate from [`make_symlink`], and why Windows uses a junction
///
/// [`make_symlink`] makes a FILE symlink, which on Windows needs Developer Mode
/// or elevation — so on an ordinary Windows machine it returns `false` and its
/// callers SKIP. A **junction** needs no privilege at all, and measured on this
/// project's own Windows box it is exactly the input a directory walk has to
/// defend against:
///
/// * `symlink_metadata(link).file_type().is_symlink()` — **`true`**
/// * `Path::is_dir(link)` — **`true`**, because it FOLLOWS the link
///
/// Those two facts are the whole test: a walk branching on `is_dir()` descends
/// into it, and a walk that inspects the link itself does not.
///
/// A junction is **not** a substitute in [`crate::payload`]'s case, and that
/// difference is real rather than an inconsistency: that walker needs a `.rs`
/// FILE link, and `symlink_metadata` reports `is_dir() == false` for a
/// junction, so it reaches neither of that walker's branches. Here the input
/// under test IS a directory link, which is precisely what a junction is.
///
/// # Parameters
///
/// * `link` — the path to create.
/// * `target` — an existing directory the link should point at.
///
/// # Returns
///
/// `true` if the link exists afterwards; `false` if the OS refused to create
/// one. A caller that gets `false` must report the property UNVERIFIED, never
/// passed — a fact about the machine must not become a green test.
///
/// # Panics
///
/// On Unix, on any failure that is **not** a privilege refusal.
pub fn make_dir_link(link: PathBuf, target: PathBuf) -> bool {
    #[cfg(unix)]
    {
        match std::os::unix::fs::symlink(&target, &link) {
            Ok(()) => true,
            Err(e) if is_privilege_refusal(&e) => false,
            Err(e) => panic!("failed to create dir symlink {link:?} -> {target:?}: {e}"),
        }
    }
    // `mklink` is a `cmd` builtin, so it cannot be spawned directly. The exit
    // status is not trusted on its own: what the caller needs to know is
    // whether the link EXISTS, so that is what is answered — a `cmd` that is
    // missing, or refuses, degrades to "could not create" and the caller SKIPs.
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&link)
            .arg(&target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        std::fs::symlink_metadata(&link).is_ok()
    }
}

/// A minimal HTTP/1.1 responder for proxy tests (Task 4 and later): reads the
/// full request body, discards it, and always answers `200 OK` with a fixed
/// tiny body. It exists so `SpyProxy` tests have something real to forward to
/// without depending on a live backend.
///
/// There is no shutdown handle and no request inspection — the two proxy
/// tests that use this only care that a POST reaches SOME server and gets
/// SOME response back; the proxy's own recording is what they actually
/// assert on. The OS reclaims the ephemeral port when the test process exits.
pub struct EchoServer {
    addr: std::net::SocketAddr,
    received: Arc<AtomicUsize>,
}

impl EchoServer {
    /// The base URL a client (or a proxy under test) should send requests to.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// How many requests this server has actually received.
    ///
    /// It exists for the one property that cannot be observed from the proxy's
    /// own side: that a request was **not** forwarded. Reading the proxy's
    /// registry cannot answer that — a proxy that forwarded a corrupted body
    /// and one that forwarded nothing can leave the registry looking the same —
    /// so the question has to be put to the far end.
    pub fn received(&self) -> usize {
        self.received.load(Ordering::SeqCst)
    }
}

/// Binds on an ephemeral port and serves the fixed echo response until the
/// test process exits.
///
/// # Panics
///
/// Panics if the ephemeral port cannot be bound. Acceptable here: this is
/// `#[cfg(test)]`-only fixture setup (see the module doc), and a setup
/// failure should stop the test immediately rather than run against a proxy
/// with nothing behind it.
pub async fn spawn_echo_server() -> EchoServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind echo server");
    let addr = listener.local_addr().expect("echo server local address");
    let received = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&received);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let counter = Arc::clone(&counter);
            tokio::spawn(async move {
                let io = hyper_util::rt::TokioIo::new(stream);
                let svc = hyper::service::service_fn(
                    move |req: hyper::Request<hyper::body::Incoming>| {
                        let counter = Arc::clone(&counter);
                        async move {
                            // Counted BEFORE the body is drained: what the
                            // caller asks is whether the request ARRIVED at
                            // all, and a request that arrives and then fails
                            // to be read still arrived.
                            counter.fetch_add(1, Ordering::SeqCst);
                            // Drain the body so the connection completes
                            // cleanly; its content is irrelevant to what the
                            // proxy tests check — they read the PROXY's
                            // record, not this server's reply.
                            let _ = http_body_util::BodyExt::collect(req.into_body()).await;
                            Ok::<_, std::convert::Infallible>(hyper::Response::new(
                                http_body_util::Full::new(hyper::body::Bytes::from_static(b"ok")),
                            ))
                        }
                    },
                );
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });
    EchoServer { addr, received }
}

/// The body length a truncated request PROMISES but never delivers in full.
const TRUNCATED_REQUEST_CONTENT_LENGTH: usize = 1000;

/// The body bytes a truncated request actually writes before closing its write
/// half — deliberately far short of [`TRUNCATED_REQUEST_CONTENT_LENGTH`].
const TRUNCATED_REQUEST_ACTUAL_BODY: &[u8] = b"0123456789";

/// Sends a `POST` whose `Content-Length` promises more body than it delivers,
/// then closes the write half — the CLIENT-side mirror of
/// [`spawn_truncating_server`], and the only way to make a server's request
/// body read genuinely fail.
///
/// Deliberately bypasses `reqwest` and `hyper` on the client side, for the same
/// reason [`spawn_truncating_server`] bypasses `hyper` on the server side: both
/// libraries keep a `Content-Length` and the bytes they write in sync by
/// construction, so there is no way to ask either of them to lie. Producing the
/// failure means writing the wire bytes by hand.
///
/// It then reads the connection to EOF, which is what makes the caller's
/// assertions safe to make: the server holds the socket open for as long as it
/// is handling the request, so EOF means the handler has finished with it.
///
/// # Parameters
///
/// * `base_url` — the server's base URL, as [`EchoServer::url`] renders it.
/// * `path` — the request path, e.g. `/api/chat`.
///
/// # Panics
///
/// Panics if the connection cannot be opened or written. Acceptable here: this
/// is `#[cfg(test)]`-only fixture code, and a setup failure should stop the
/// test rather than let it assert against a request that was never sent.
pub async fn send_truncated_request(base_url: &str, path: &str) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let addr = base_url
        .strip_prefix("http://")
        .expect("the harness only ever serves plaintext http");
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("connect to the server under test");
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\n\
         Content-Length: {TRUNCATED_REQUEST_CONTENT_LENGTH}\r\n\r\n"
    );
    stream
        .write_all(head.as_bytes())
        .await
        .expect("write the request head");
    stream
        .write_all(TRUNCATED_REQUEST_ACTUAL_BODY)
        .await
        .expect("write the short body");
    stream
        .shutdown()
        .await
        .expect("close the write half, so the server sees EOF mid-body");
    // Read to EOF. Whatever comes back is irrelevant — the point is to wait
    // until the server is done with this connection, so the caller is not
    // racing the handler it means to observe.
    let mut sink = Vec::new();
    let _ = stream.read_to_end(&mut sink).await;
}

/// A raw TCP responder that promises more body than it ever sends, then
/// closes the connection — used to exercise "the upstream answered but the
/// body could not be read back in full" without depending on any crate
/// beyond `tokio`.
///
/// Deliberately bypasses `hyper` on the SERVER side: a `hyper` server given a
/// complete, known body (as [`spawn_echo_server`] uses) keeps its
/// `Content-Length` header and the bytes it actually writes in sync by
/// construction, so there is no way to make it lie. Producing a genuine
/// truncated-body failure means writing the wire bytes by hand — a
/// `Content-Length` the body never reaches, followed by closing the socket.
pub struct TruncatingServer {
    addr: std::net::SocketAddr,
}

impl TruncatingServer {
    /// The base URL a client (or a proxy under test) should send requests to.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// The body length this server promises but never delivers in full.
const TRUNCATING_SERVER_CONTENT_LENGTH: usize = 1000;

/// The number of body bytes this server actually writes before closing the
/// connection — deliberately far short of
/// [`TRUNCATING_SERVER_CONTENT_LENGTH`].
const TRUNCATING_SERVER_ACTUAL_BODY: &[u8] = b"0123456789";

/// Binds on an ephemeral port and, for every connection, writes a `200 OK`
/// with `Content-Length: 1000` followed by only 10 body bytes, then closes
/// the socket — a response no HTTP/1.1 reader can complete without erroring.
///
/// # Panics
///
/// Panics if the ephemeral port cannot be bound. Acceptable here: this is
/// `#[cfg(test)]`-only fixture setup (see the module doc), and a setup
/// failure should stop the test immediately rather than run against a proxy
/// with nothing behind it.
pub async fn spawn_truncating_server() -> TruncatingServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind truncating server");
    let addr = listener
        .local_addr()
        .expect("truncating server local address");
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                // Drain (ignore) whatever the client sent — replying does
                // not require parsing it.
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {TRUNCATING_SERVER_CONTENT_LENGTH}\r\n\r\n"
                );
                let _ = stream.write_all(head.as_bytes()).await;
                let _ = stream.write_all(TRUNCATING_SERVER_ACTUAL_BODY).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    TruncatingServer { addr }
}

// --- Task 8: preflight helpers ---------------------------------------------
//
// `fresh_temp_dir`, `temp_root_with` and `repo_where_the_negation_was_removed`
// deliberately return a bare `PathBuf`, NOT a `TempDir`: the production code
// they feed (`check_lock_is_tracked`, `sweep_stale_temps`) takes `&Path`
// directly, and `TempDir` has no `Deref<Target = Path>` for that reference to
// coerce through. The directories are never cleaned up — accepted the same
// way `TempDir`'s own doc accepts it for the OS-level fallback: they are tiny
// scaffolding under the OS temp root, and [`UNIQUE`] guarantees the next call
// never collides with what this one leaves behind.

/// A fresh, uniquely-named directory under the OS temp root, without
/// automatic cleanup. See the module note above for why the callers below
/// need a bare [`PathBuf`] instead of a self-cleaning [`TempDir`].
///
/// # Panics
///
/// Panics if the directory cannot be created. Acceptable here: this is
/// `#[cfg(test)]`-only fixture setup, and a setup failure should stop the
/// test immediately rather than run against a partial tree.
fn fresh_temp_dir(prefix: &str) -> PathBuf {
    let unique = UNIQUE.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("failed to create temp directory");
    dir
}

/// Builds a scratch parent directory containing one throwaway, EMPTY
/// subdirectory per `(name, _)` pair, named EXACTLY as given.
///
/// The second element of each pair is not read by this function — it exists
/// in the test data purely to document, next to the name, what the caller
/// EXPECTS [`crate::preflight::sweep_stale_temps`] to do with that entry. The
/// name itself is what actually drives the behaviour under test: a name whose
/// embedded PID parses and is dead gets deleted, a name whose PID is alive or
/// does not parse at all (as `SELF` deliberately does not — the sweep's own
/// contract is to leave an unparseable name alone, "not ours to judge") is
/// left standing.
pub fn temp_root_with(entries: &[(&str, bool)]) -> PathBuf {
    let root = fresh_temp_dir("magi-smoke-sweep-test");
    for (name, _expected_to_survive) in entries {
        std::fs::create_dir_all(root.join(name)).expect("create sweep-test entry");
    }
    root
}

/// Builds a throwaway git repository whose `smoke/Cargo.lock` is untracked —
/// the state after the `!smoke/Cargo.lock` negation in `.gitignore` stops
/// applying (R8).
///
/// `git ls-files --error-unmatch` needs no commit history to answer
/// truthfully: a freshly initialised repo has an empty index, so the command
/// fails on any path — which is exactly the state
/// [`crate::preflight::check_lock_is_tracked`] exists to catch.
///
/// # Panics
///
/// Panics if the fixture files or `git init` fail. Acceptable here: this is
/// `#[cfg(test)]`-only fixture setup, and a setup failure should stop the
/// test immediately rather than run against a partial repo.
pub fn repo_where_the_negation_was_removed() -> PathBuf {
    let dir = fresh_temp_dir("magi-smoke-lock-test");
    // The `.gitignore` content is narrative, not load-bearing: `git ls-files`
    // does not consult it at all. What actually reproduces R8's failure is
    // that `smoke/Cargo.lock` is written to disk but never `git add`ed.
    std::fs::write(dir.join(".gitignore"), "target/\nCargo.lock\n")
        .expect("write fixture .gitignore");
    std::fs::create_dir_all(dir.join("smoke")).expect("create fixture smoke/ dir");
    std::fs::write(
        dir.join("smoke/Cargo.lock"),
        "# never staged, so never tracked",
    )
    .expect("write fixture Cargo.lock");
    let out = std::process::Command::new("git")
        .arg("init")
        .current_dir(&dir)
        .output()
        .expect("git init for the fixture repo");
    assert!(
        out.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

/// One request as a stub received it: what it was, where it went, and what it
/// carried.
///
/// The body is kept whole rather than hashed, unlike the spy proxy's own
/// record: the question here is *what did we ASK the backend to do*, and a hash
/// can only confirm a body somebody already knows. A probe that stopped naming a
/// model, or stopped bounding its output, has to be readable as that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenRequest {
    /// The HTTP method, e.g. `"GET"` or `"POST"`.
    pub method: String,
    /// The path, without the host.
    pub path: String,
    /// The request body, as sent.
    pub body: String,
}

/// An HTTP stub that answers everything immediately and REMEMBERS what it was
/// asked.
///
/// It exists for the one property no timing-based stub can show: that the
/// contention probe puts a real completion on the wire. A stub that only counts
/// requests, or only delays them, is satisfied just as well by a manifest
/// listing — which is exactly how a probe that could not detect contention
/// passed its own tests.
pub struct RecordingStub {
    addr: std::net::SocketAddr,
    seen: Arc<std::sync::Mutex<Vec<SeenRequest>>>,
}

impl RecordingStub {
    /// The base URL a client (or `probe`) should send requests to.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Every request received so far, in arrival order.
    ///
    /// # Panics
    ///
    /// Panics if the lock is poisoned — which means a serving task panicked
    /// while holding it, and a test reading a half-written log would be worse
    /// than one that stops.
    pub fn seen(&self) -> Vec<SeenRequest> {
        self.seen.lock().expect("recording stub lock").clone()
    }
}

/// Binds on an ephemeral port and answers `{}` to everything, recording the
/// method, path and body of each request for the life of the test process.
///
/// # Panics
///
/// Panics if the ephemeral port cannot be bound. Acceptable here: this is
/// `#[cfg(test)]`-only fixture setup, and a setup failure should stop the test
/// immediately rather than run against a stub with nothing behind it.
pub async fn stub_that_records_requests() -> RecordingStub {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind recording stub");
    let addr = listener.local_addr().expect("recording stub local address");
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let log = Arc::clone(&log);
            tokio::spawn(async move {
                let io = hyper_util::rt::TokioIo::new(stream);
                let svc = hyper::service::service_fn(
                    move |req: hyper::Request<hyper::body::Incoming>| {
                        let log = Arc::clone(&log);
                        async move {
                            let method = req.method().to_string();
                            let path = req.uri().path().to_string();
                            let body = http_body_util::BodyExt::collect(req.into_body())
                                .await
                                .map(|b| String::from_utf8_lossy(&b.to_bytes()).into_owned())
                                .unwrap_or_default();
                            if let Ok(mut l) = log.lock() {
                                l.push(SeenRequest { method, path, body });
                            }
                            Ok::<_, std::convert::Infallible>(hyper::Response::new(
                                http_body_util::Full::new(hyper::body::Bytes::from_static(b"{}")),
                            ))
                        }
                    },
                );
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });
    RecordingStub { addr, seen }
}

/// An HTTP stub whose FIRST request is answered only after a deliberate
/// delay, and every later one immediately — the shape of a model that must
/// load once and is fast forever after. Proves
/// [`crate::preflight::probe`]'s retry recovers from exactly that case.
pub struct SlowOnceStub {
    addr: std::net::SocketAddr,
    attempts: Arc<AtomicUsize>,
}

impl SlowOnceStub {
    /// The base URL a client (or `probe`) should send requests to.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Total requests this stub has received so far.
    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }
}

/// How long the FIRST request is held before answering. Comfortably longer
/// than the 50ms window `probe`'s tests use for their FIRST attempt, so that
/// attempt reliably times out; every later request answers immediately, so
/// the widened second window never has to absorb this delay at all.
const SLOW_ONCE_DELAY: Duration = Duration::from_millis(300);

/// Binds on an ephemeral port and serves until the test process exits: the
/// first request received is delayed by [`SLOW_ONCE_DELAY`], every
/// subsequent one answers immediately.
///
/// # Panics
///
/// Panics if the ephemeral port cannot be bound. Acceptable here: this is
/// `#[cfg(test)]`-only fixture setup, and a setup failure should stop the
/// test immediately rather than run against a stub with nothing behind it.
pub async fn stub_that_is_slow_on_first_request_only() -> SlowOnceStub {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind slow-once stub");
    let addr = listener.local_addr().expect("slow-once stub local address");
    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&attempts);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let counter = Arc::clone(&counter);
            tokio::spawn(async move {
                let io = hyper_util::rt::TokioIo::new(stream);
                let svc = hyper::service::service_fn(
                    move |req: hyper::Request<hyper::body::Incoming>| {
                        let counter = Arc::clone(&counter);
                        async move {
                            let _ = http_body_util::BodyExt::collect(req.into_body()).await;
                            let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
                            if n == 1 {
                                tokio::time::sleep(SLOW_ONCE_DELAY).await;
                            }
                            Ok::<_, std::convert::Infallible>(hyper::Response::new(
                                http_body_util::Full::new(hyper::body::Bytes::from_static(b"{}")),
                            ))
                        }
                    },
                );
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });
    SlowOnceStub { addr, attempts }
}

/// An HTTP stub that delays EVERY request past both of `probe`'s windows —
/// the shape of a genuinely saturated endpoint, as opposed to
/// [`SlowOnceStub`]'s one-time cold-start delay.
pub struct AlwaysSlowStub {
    addr: std::net::SocketAddr,
}

impl AlwaysSlowStub {
    /// The base URL a client (or `probe`) should send requests to.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// Longer than `probe`'s WIDENED window (`50ms * PROBE_RETRY_FACTOR =
/// 150ms`) as well as its first, so neither attempt can succeed against this
/// stub.
const ALWAYS_SLOW_DELAY: Duration = Duration::from_millis(500);

/// Binds on an ephemeral port and answers every request only after
/// [`ALWAYS_SLOW_DELAY`], for the life of the test process.
///
/// # Panics
///
/// Panics if the ephemeral port cannot be bound. Acceptable here: this is
/// `#[cfg(test)]`-only fixture setup, and a setup failure should stop the
/// test immediately rather than run against a stub with nothing behind it.
pub async fn stub_that_is_always_slow() -> AlwaysSlowStub {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind always-slow stub");
    let addr = listener
        .local_addr()
        .expect("always-slow stub local address");
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            tokio::spawn(async move {
                let io = hyper_util::rt::TokioIo::new(stream);
                let svc = hyper::service::service_fn(
                    |req: hyper::Request<hyper::body::Incoming>| async move {
                        let _ = http_body_util::BodyExt::collect(req.into_body()).await;
                        tokio::time::sleep(ALWAYS_SLOW_DELAY).await;
                        Ok::<_, std::convert::Infallible>(hyper::Response::new(
                            http_body_util::Full::new(hyper::body::Bytes::from_static(b"{}")),
                        ))
                    },
                );
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });
    AlwaysSlowStub { addr }
}

/// An HTTP stub that answers `GET /api/tags` with a model listing and `{}` to
/// everything else, immediately.
pub struct ListingStub {
    addr: std::net::SocketAddr,
}

impl ListingStub {
    /// The base URL a client (or the preflight) should send requests to.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// Binds on an ephemeral port and answers the reachability path with the
/// Ollama-shaped listing `{"models":[{"name": ..}, ..]}` naming exactly
/// `models`, and `{}` to every other path.
///
/// It exists because an echo or `{}` stub establishes NOTHING about which
/// models a backend holds, so a preflight check over that listing cannot be
/// observed against one: the test would pass whether or not the check ran.
///
/// # Parameters
///
/// * `models` — the model names the backend is to claim it holds.
///
/// # Panics
///
/// Panics if the ephemeral port cannot be bound. Acceptable here: this is
/// `#[cfg(test)]`-only fixture setup, and a setup failure should stop the
/// test immediately rather than run against a stub with nothing behind it.
pub async fn stub_that_lists_models(models: &[&str]) -> ListingStub {
    let listing = serde_json::json!({
        "models": models.iter().map(|m| serde_json::json!({ "name": m })).collect::<Vec<_>>(),
    })
    .to_string();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listing stub");
    let addr = listener.local_addr().expect("listing stub local address");
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let listing = listing.clone();
            tokio::spawn(async move {
                let io = hyper_util::rt::TokioIo::new(stream);
                let svc = hyper::service::service_fn(
                    move |req: hyper::Request<hyper::body::Incoming>| {
                        let listing = listing.clone();
                        async move {
                            let tags = req.uri().path() == "/api/tags";
                            let _ = http_body_util::BodyExt::collect(req.into_body()).await;
                            let body = if tags { listing } else { "{}".to_string() };
                            Ok::<_, std::convert::Infallible>(hyper::Response::new(
                                http_body_util::Full::new(hyper::body::Bytes::from(body)),
                            ))
                        }
                    },
                );
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });
    ListingStub { addr }
}

/// An HTTP stub that answers everything with `404 Not Found`, immediately.
pub struct NotFoundStub {
    addr: std::net::SocketAddr,
}

impl NotFoundStub {
    /// The base URL a client (or `probe`) should send requests to.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// Binds on an ephemeral port and answers `404` to everything, instantly.
///
/// It is the shape of an Ollama asked for a model it does not hold: the
/// request is rejected on inspection, WITHOUT generating, so it never enters
/// the inference queue. That is why a fast answer from it proves nothing about
/// contention, and why the probe has to say so instead of reporting clear.
///
/// # Panics
///
/// Panics if the ephemeral port cannot be bound. Acceptable here: this is
/// `#[cfg(test)]`-only fixture setup, and a setup failure should stop the
/// test immediately rather than run against a stub with nothing behind it.
pub async fn stub_that_holds_no_model() -> NotFoundStub {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind not-found stub");
    let addr = listener.local_addr().expect("not-found stub local address");
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            tokio::spawn(async move {
                let io = hyper_util::rt::TokioIo::new(stream);
                let svc = hyper::service::service_fn(
                    |req: hyper::Request<hyper::body::Incoming>| async move {
                        let _ = http_body_util::BodyExt::collect(req.into_body()).await;
                        let mut resp = hyper::Response::new(http_body_util::Full::new(
                            hyper::body::Bytes::from_static(b"{\"error\":\"model not found\"}"),
                        ));
                        *resp.status_mut() = hyper::StatusCode::NOT_FOUND;
                        Ok::<_, std::convert::Infallible>(resp)
                    },
                );
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });
    NotFoundStub { addr }
}

/// Runs the REAL preflight, end to end, with `break_proxy = true` — proves
/// `S20`: a broken proxy is reported as "cannot test", never as a scenario
/// failure.
///
/// Config, fixtures, workspace and lock all run against the REAL repository
/// tree (see `paths.rs`), which is exactly what makes this prove the FULL
/// pipeline instead of a stand-in for it; only the backend is a stub, because
/// a probe against a real Ollama is not this test's concern.
pub async fn run_with_broken_proxy(
) -> Result<crate::preflight::Announcement, crate::preflight::PreflightError> {
    let upstream = spawn_echo_server().await;
    let cfg = crate::config::Config {
        endpoint: upstream.url(),
        ..crate::config::Config::default()
    };
    crate::preflight::run(&cfg, &[], true, false).await
}

/// Port 1 is reserved and nothing listens on it, so a connection to it is
/// refused immediately on both platforms the project builds on — no timeout to
/// wait out, and no dependence on a stub the test would have to keep alive.
const UNREACHABLE_ENDPOINT: &str = "http://127.0.0.1:1";

/// Runs the REAL preflight against an endpoint that answers nothing, with
/// `no_backend` as given — the pair of runs that proves `--no-backend` skips
/// the two backend-dependent steps and ONLY skips them.
///
/// Everything else runs against the real repository tree, exactly as
/// [`run_with_broken_proxy`] does, so this exercises the whole pipeline rather
/// than a stand-in for it.
///
/// # Parameters
///
/// * `no_backend` — the flag under test.
pub async fn run_against_an_unreachable_backend(
    no_backend: bool,
) -> Result<crate::preflight::Announcement, crate::preflight::PreflightError> {
    let cfg = crate::config::Config {
        endpoint: UNREACHABLE_ENDPOINT.to_string(),
        ..crate::config::Config::default()
    };
    crate::preflight::run(&cfg, &[], false, no_backend).await
}
