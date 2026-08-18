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
}

impl EchoServer {
    /// The base URL a client (or a proxy under test) should send requests to.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
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
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            tokio::spawn(async move {
                let io = hyper_util::rt::TokioIo::new(stream);
                let svc = hyper::service::service_fn(
                    |req: hyper::Request<hyper::body::Incoming>| async {
                        // Drain the body so the connection completes cleanly;
                        // its content is irrelevant to what the proxy tests
                        // check — they read the PROXY's record, not this
                        // server's reply.
                        let _ = http_body_util::BodyExt::collect(req.into_body()).await;
                        Ok::<_, std::convert::Infallible>(hyper::Response::new(
                            http_body_util::Full::new(hyper::body::Bytes::from_static(b"ok")),
                        ))
                    },
                );
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });
    EchoServer { addr }
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
    crate::preflight::run(&cfg, &[], true).await
}
