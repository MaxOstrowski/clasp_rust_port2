//! Rust port of `original_clasp/clasp/config.h.in`.

pub const CLASP_VERSION: &str = "4.0.0";
pub const CLASP_VERSION_MAJOR: u32 = 4;
pub const CLASP_VERSION_MINOR: u32 = 0;
pub const CLASP_VERSION_PATCH: u32 = 0;
pub const CLASP_LEGAL: &str = "Copyright (C) Benjamin Kaufmann";
pub const CLASP_HAS_THREADS: u32 = 1;
pub const CLASP_USE_STD_VECTOR: u32 = 0;

#[allow(non_upper_case_globals)]
pub const cache_line_size: usize = 64;
