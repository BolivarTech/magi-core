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
use std::sync::atomic::{AtomicU64, Ordering};

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
