// TARGET: error.rs
// EXPECT: must construct External and nothing else
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
fn other() -> Self { Self::Auth { f: 1 } }
