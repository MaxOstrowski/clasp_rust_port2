use rust_clasp::clasp::clasp_facade::{SolveMode, SolveResult, SolveResultExt, SolveStatus};

#[test]
fn solve_result_matches_upstream_status_queries() {
    let unknown = SolveResult::default();
    assert!(unknown.unknown());
    assert!(!unknown.sat());
    assert!(!unknown.unsat());
    assert!(!unknown.exhausted());
    assert!(!unknown.interrupted());
    assert_eq!(unknown.status(), SolveStatus::Unknown);

    let sat = SolveResult::new(SolveStatus::Sat as u8, 0);
    assert!(sat.sat());
    assert!(!sat.unsat());
    assert!(!sat.unknown());
    assert_eq!(SolveStatus::from(sat), SolveStatus::Sat);

    let unsat = SolveResult::new(SolveStatus::Unsat as u8, 0);
    assert!(unsat.unsat());
    assert!(!unsat.sat());
    assert!(!unsat.unknown());
    assert_eq!(unsat.status(), SolveStatus::Unsat);
}

#[test]
fn solve_result_preserves_extended_flags_without_changing_base_status() {
    let result = SolveResult::new(
        SolveStatus::Sat as u8 | SolveResultExt::Exhaust as u8 | SolveResultExt::Interrupt as u8,
        14,
    );

    assert!(result.sat());
    assert!(result.exhausted());
    assert!(result.interrupted());
    assert_eq!(result.status(), SolveStatus::Sat);
    assert_eq!(result.signal, 14);

    let unknown = SolveResult::new(SolveResultExt::Interrupt as u8, 9);
    assert!(unknown.unknown());
    assert!(unknown.interrupted());
    assert_eq!(unknown.status(), SolveStatus::Unknown);
}

#[test]
fn solve_mode_matches_upstream_bitmask_semantics() {
    assert!(SolveMode::DEF.is_default());
    assert_eq!(SolveMode::DEF.bits(), 0);
    assert_eq!(SolveMode::ASYNC.bits(), 1);
    assert_eq!(SolveMode::YIELD.bits(), 2);
    assert_eq!(SolveMode::ASYNC_YIELD.bits(), 3);

    let mode = SolveMode::ASYNC | SolveMode::YIELD;
    assert_eq!(mode, SolveMode::ASYNC_YIELD);
    assert!(mode.contains(SolveMode::ASYNC));
    assert!(mode.contains(SolveMode::YIELD));
    assert!(!SolveMode::ASYNC.contains(SolveMode::YIELD));

    let mut toggled = mode;
    toggled ^= SolveMode::ASYNC;
    assert_eq!(toggled, SolveMode::YIELD);

    toggled &= SolveMode::ASYNC_YIELD;
    assert_eq!(toggled, SolveMode::YIELD);

    toggled |= SolveMode::ASYNC;
    assert_eq!(toggled, SolveMode::ASYNC_YIELD);
}
