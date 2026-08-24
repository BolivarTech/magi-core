// TARGET: error.rs
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
pub fn build() -> Self { Self::External { f: 0 } }
// A crate-private constructor building a SECOND variant. NOT a door: `#[non_exhaustive]`
// already stops another crate from writing the literal, so this must be ACCEPTED. Without
// this fixture, widening rule 8b back to "every constructor in the file" passes the suite --
// the accept side of the change would be pinned by nothing.
pub(crate) fn bound_it() -> Self { Self::Http { f: 0 } }
