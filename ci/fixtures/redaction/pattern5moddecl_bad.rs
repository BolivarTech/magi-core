// TARGET: providers/subject.rs
// EXPECT: raw error interpolation
// A module DECLARATION has no body, so there is nothing to skip. Treating it as the start of a
// skip ran until the next unrelated column-0 `}` and blanked real production code.
use reqwest as _;
#[cfg(test)]
mod tests;
impl P {
    pub fn new() -> Self {
        let e = ProviderError::Auth { message: format!("{e}") };
    }
}
