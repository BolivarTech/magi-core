// TARGET: error.rs
//! `Self::Auth{` without the space is ordinary Rust, and it is a second door into a variant the
//! outside world must not be able to build.
pub enum ProviderError {
    #[non_exhaustive]
    Http {
        f: u8,
    },
    #[non_exhaustive]
    Network {
        f: u8,
    },
    #[non_exhaustive]
    Timeout {
        f: u8,
    },
    #[non_exhaustive]
    Auth {
        f: u8,
    },
    #[non_exhaustive]
    Process {
        f: u8,
    },
    #[non_exhaustive]
    ResponseTooLarge {
        f: u8,
    },
    #[non_exhaustive]
    RetryAbandoned {
        f: u8,
    },
    #[non_exhaustive]
    External {
        f: u8,
    },
}
fn build() -> Self { Self::External { f: 0 } }
