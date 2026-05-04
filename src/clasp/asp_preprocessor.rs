//! Minimal Rust boundary for `SatPreprocessor` used by the solver/shared-context setup path.

#[derive(Debug, Default)]
pub struct SatPreprocessor;

impl SatPreprocessor {
    pub const fn new() -> Self {
        Self
    }
}
