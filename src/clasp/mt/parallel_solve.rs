//! Partial Rust port of `original_clasp/clasp/mt/parallel_solve.h` and
//! `original_clasp/src/parallel_solve.cpp`.

use crate::clasp::mt::thread::Thread;
use crate::clasp::solver_strategies::ScheduleStrategy;
use crate::clasp::solver_types::SolverSet;
use crate::potassco::bits::{bit_floor, bit_max, toggle_bit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParallelSearchMode {
    Split,
    Compete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParallelAlgorithmOptions {
    pub threads: u32,
    pub mode: ParallelSearchMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParallelIntegrationTopology {
    All,
    Ring,
    Cube,
    CubeX,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParallelDistributionPolicy {
    pub size: u32,
    pub lbd: u32,
    pub types: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParallelDistributionMode {
    Global,
    Local,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParallelDistributionOptions {
    pub policy: ParallelDistributionPolicy,
    pub mode: ParallelDistributionMode,
}

impl Default for ParallelDistributionMode {
    fn default() -> Self {
        Self::Global
    }
}

impl Default for ParallelDistributionOptions {
    fn default() -> Self {
        Self::new(ParallelDistributionMode::Global)
    }
}

impl ParallelDistributionOptions {
    #[must_use]
    pub fn new(mode: ParallelDistributionMode) -> Self {
        Self {
            policy: ParallelDistributionPolicy::default(),
            mode,
        }
    }

    #[must_use]
    pub fn policy(&self) -> &ParallelDistributionPolicy {
        &self.policy
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParallelGlobalRestarts {
    pub max_restarts: u32,
    pub schedule: ScheduleStrategy,
}

impl Default for ParallelGlobalRestarts {
    fn default() -> Self {
        Self::new()
    }
}

impl ParallelGlobalRestarts {
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_restarts: 0,
            schedule: ScheduleStrategy::default(),
        }
    }
}

pub struct ParallelSolveOptions {
    pub algorithm: ParallelAlgorithmOptions,
}

impl ParallelSolveOptions {
    #[must_use]
    pub fn default_portfolio(&self) -> bool {
        self.algorithm.mode == ParallelSearchMode::Compete
    }

    #[must_use]
    pub fn recommended_solvers() -> u32 {
        Thread::<()>::hardware_concurrency()
    }

    #[must_use]
    pub const fn supported_solvers() -> u32 {
        64
    }

    #[must_use]
    pub fn num_solver(&self) -> u32 {
        self.algorithm.threads
    }

    pub fn set_solvers(&mut self, num_solvers: u32) {
        self.algorithm.threads = num_solvers.max(1);
    }

    #[must_use]
    pub fn init_peer_set(
        solver_id: u32,
        topology: ParallelIntegrationTopology,
        num_threads: u32,
    ) -> SolverSet {
        if topology == ParallelIntegrationTopology::All {
            return Self::full_peer_set(solver_id, num_threads);
        }
        if topology == ParallelIntegrationTopology::Ring {
            let prev = if solver_id > 0 {
                solver_id
            } else {
                num_threads
            } - 1;
            let next = (solver_id + 1) % num_threads;
            return SolverSet::from([prev, next]);
        }
        let n = num_threads;
        let k = bit_floor(num_threads);
        let ext = if topology == ParallelIntegrationTopology::CubeX && k != n {
            k
        } else {
            0
        };
        let s = if (k ^ solver_id) >= n {
            k ^ solver_id
        } else {
            k * 2
        };
        let mut result = SolverSet::new();
        let mut mask = 1_u32;
        while mask <= k {
            let pos = mask ^ solver_id;
            if pos < n {
                result.add(pos);
            }
            if mask < ext {
                let r = mask ^ s;
                if r < n {
                    result.add(r);
                }
                if pos >= n {
                    result.add(pos ^ k);
                }
            }
            mask *= 2;
        }
        debug_assert!(!result.contains(solver_id));
        result
    }

    #[must_use]
    pub fn full_peer_set(solver_id: u32, num_threads: u32) -> SolverSet {
        SolverSet::from_rep(toggle_bit(bit_max::<u64>(num_threads), solver_id))
    }
}
