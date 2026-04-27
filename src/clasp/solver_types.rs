//! Partial Rust port of `original_clasp/clasp/solver_types.h` and
//! `original_clasp/src/solver_types.cpp`.
//!
//! The upstream `solver_types` unit mixes two Bundle A runtime seams: solver
//! statistics aggregates and low-level assignment/watch/reason storage.
//! Keep the public surface stable here while the implementation lives in
//! smaller internal modules that follow that seam more closely.

#[path = "solver_types/runtime.rs"]
mod runtime;
#[path = "solver_types/stats.rs"]
mod stats;

pub use crate::clasp::statistics::{
    StatisticArray, StatisticMap, StatisticObject, StatisticType, StatisticValue,
};
pub use runtime::{
    Assignment, ClauseWatch, ClauseWatchEqHead, GenericWatch, GenericWatchEqConstraint,
    ImpliedList, ImpliedLiteral, ReasonStore32, ReasonStore32Value, ReasonStore64,
    ReasonStore64Value, ReasonVec, ReasonWithData, SolverSet, ValueSet, WatchList, release_vec,
    releaseVec,
};
pub use stats::{CoreStats, ExtendedStats, JumpStats, SolverStats};
