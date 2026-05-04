use rust_clasp::clasp::mt::parallel_solve::{
    ParallelAlgorithmOptions, ParallelDistributionMode, ParallelDistributionOptions,
    ParallelGlobalRestarts, ParallelIntegrationTopology, ParallelSearchMode, ParallelSolveOptions,
};
use rust_clasp::clasp::mt::thread::Thread;
use rust_clasp::clasp::solver_strategies::ScheduleStrategy;

#[test]
fn default_portfolio_is_true_only_for_competition_mode() {
    let compete = ParallelSolveOptions {
        algorithm: ParallelAlgorithmOptions {
            threads: 1,
            mode: ParallelSearchMode::Compete,
        },
    };
    let split = ParallelSolveOptions {
        algorithm: ParallelAlgorithmOptions {
            threads: 1,
            mode: ParallelSearchMode::Split,
        },
    };

    assert!(compete.default_portfolio());
    assert!(!split.default_portfolio());
}

#[test]
fn num_solver_reads_the_configured_parallel_thread_count() {
    let opts = ParallelSolveOptions {
        algorithm: ParallelAlgorithmOptions {
            threads: 6,
            mode: ParallelSearchMode::Compete,
        },
    };

    assert_eq!(opts.num_solver(), 6);
}

#[test]
fn recommended_solvers_matches_thread_hardware_concurrency() {
    assert_eq!(
        ParallelSolveOptions::recommended_solvers(),
        Thread::<()>::hardware_concurrency()
    );
}

#[test]
fn set_solvers_clamps_the_thread_count_to_at_least_one() {
    let mut opts = ParallelSolveOptions {
        algorithm: ParallelAlgorithmOptions {
            threads: 4,
            mode: ParallelSearchMode::Compete,
        },
    };

    opts.set_solvers(0);
    assert_eq!(opts.num_solver(), 1);

    opts.set_solvers(7);
    assert_eq!(opts.num_solver(), 7);
}

#[test]
fn supported_solvers_matches_the_upstream_fixed_limit() {
    assert_eq!(ParallelSolveOptions::supported_solvers(), 64);
}

#[test]
fn distribution_constructor_defaults_policy_and_accepts_requested_mode() {
    let global = ParallelDistributionOptions::new(ParallelDistributionMode::Global);
    let local = ParallelDistributionOptions::new(ParallelDistributionMode::Local);

    assert_eq!(global.mode, ParallelDistributionMode::Global);
    assert_eq!(global.policy.size, 0);
    assert_eq!(global.policy.lbd, 0);
    assert_eq!(global.policy.types, 0);

    assert_eq!(local.mode, ParallelDistributionMode::Local);
    assert_eq!(local.policy, global.policy);
}

#[test]
fn distribution_policy_accessor_returns_the_embedded_policy() {
    let distribution = ParallelDistributionOptions::new(ParallelDistributionMode::Global);

    assert_eq!(distribution.policy(), &distribution.policy);
}

#[test]
fn global_restarts_constructor_matches_upstream_defaults() {
    let restarts = ParallelGlobalRestarts::new();

    assert_eq!(restarts.max_restarts, 0);
    assert_eq!(restarts.schedule, ScheduleStrategy::default());
}

#[test]
fn full_peer_set_clears_only_the_selected_solver_from_the_complete_mask() {
    let peers = ParallelSolveOptions::full_peer_set(2, 5);

    assert_eq!(peers.rep(), 0b1_1011);
    assert!(!peers.contains(2));
    assert!(peers.contains(0));
    assert!(peers.contains(1));
    assert!(peers.contains(3));
    assert!(peers.contains(4));
}

#[test]
fn full_peer_set_returns_empty_when_only_one_solver_exists() {
    let peers = ParallelSolveOptions::full_peer_set(0, 1);

    assert_eq!(peers.rep(), 0);
    assert!(!peers.contains(0));
}

#[test]
fn init_peer_set_matches_all_topology_by_delegating_to_full_peer_set() {
    let peers = ParallelSolveOptions::init_peer_set(1, ParallelIntegrationTopology::All, 4);

    assert_eq!(peers.rep(), ParallelSolveOptions::full_peer_set(1, 4).rep());
}

#[test]
fn init_peer_set_matches_ring_neighbors() {
    let peers = ParallelSolveOptions::init_peer_set(0, ParallelIntegrationTopology::Ring, 4);

    assert_eq!(peers.rep(), 0b1010);
    assert!(peers.contains(1));
    assert!(peers.contains(3));
}

#[test]
fn init_peer_set_matches_cube_neighbors_for_non_power_of_two_thread_counts() {
    let peers = ParallelSolveOptions::init_peer_set(0, ParallelIntegrationTopology::Cube, 5);

    assert_eq!(peers.rep(), 0b1_0110);
    assert!(peers.contains(1));
    assert!(peers.contains(2));
    assert!(peers.contains(4));
}

#[test]
fn init_peer_set_adds_cubex_extension_edges_when_cube_partner_is_missing() {
    let peers = ParallelSolveOptions::init_peer_set(4, ParallelIntegrationTopology::CubeX, 6);

    assert_eq!(peers.rep(), 0b10_0101);
    assert!(peers.contains(0));
    assert!(peers.contains(2));
    assert!(peers.contains(5));
}
