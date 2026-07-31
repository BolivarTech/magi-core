// TARGET: error.rs
// EXPECT: not found in error.rs
pub enum ProviderError {
    #[non_exhaustive]
    Transport {
        f: u8,
    },
    #[non_exhaustive]
    Http {
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
