//! Rust port of the upstream `clasp` solver.
//!
//! The C++ implementation in `original_clasp/` remains the behavioral
//! reference while the Rust port is built out incrementally according to
//! `analysis/original_clasp/porting_order.json`.

pub mod clasp;
pub mod potassco;
