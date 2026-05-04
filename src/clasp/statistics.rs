//! Partial Rust port of `original_clasp/clasp/statistics.h` and
//! `original_clasp/src/statistics.cpp`.

#[path = "statistics/object.rs"]
mod object;
#[path = "statistics/store.rs"]
mod store;

#[doc(hidden)]
pub use object::Vtab;
pub use object::{
    StatisticArray, StatisticArrayElements, StatisticMap, StatisticObject, StatisticObjectTypeId,
    StatisticType, StatisticValue,
};
pub use store::{ClaspStatistics, Operation, StatsVisitor};
