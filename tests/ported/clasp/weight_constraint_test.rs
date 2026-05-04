//! Port target for original_clasp/tests/weight_constraint_test.cpp.

use rust_clasp::clasp::literal::{WeightLiteral, pos_lit};
use rust_clasp::clasp::weight_constraint::WeightLitsRep;

#[test]
fn weight_lits_rep_sat_matches_upstream_bound_check() {
    let sat = WeightLitsRep {
        lits: &[],
        size: 0,
        bound: 0,
        reach: 3,
    };
    let negative = WeightLitsRep { bound: -2, ..sat };
    let open = WeightLitsRep { bound: 1, ..sat };

    assert!(sat.sat());
    assert!(negative.sat());
    assert!(!open.sat());
}

#[test]
fn weight_lits_rep_unsat_matches_upstream_reach_check() {
    let base = WeightLitsRep {
        lits: &[],
        size: 0,
        bound: 3,
        reach: 3,
    };
    let unsat = WeightLitsRep { reach: 2, ..base };

    assert!(!base.unsat());
    assert!(unsat.unsat());
}

#[test]
fn weight_lits_rep_open_matches_upstream_interval_check() {
    let open = WeightLitsRep {
        lits: &[],
        size: 0,
        bound: 2,
        reach: 4,
    };
    let sat = WeightLitsRep { bound: 0, ..open };
    let unsat = WeightLitsRep { reach: 1, ..open };

    assert!(open.open());
    assert!(!sat.open());
    assert!(!unsat.open());
}

#[test]
fn weight_lits_rep_has_weights_checks_first_weight_and_size() {
    let empty = WeightLitsRep {
        lits: &[],
        size: 0,
        bound: 1,
        reach: 1,
    };
    let cardinality = [WeightLiteral {
        lit: pos_lit(1),
        weight: 1,
    }];
    let weighted = [WeightLiteral {
        lit: pos_lit(1),
        weight: 2,
    }];
    let cardinality_rep = WeightLitsRep {
        lits: &cardinality,
        size: 1,
        ..empty
    };
    let weighted_rep = WeightLitsRep {
        lits: &weighted,
        size: 1,
        ..empty
    };

    assert!(!empty.has_weights());
    assert!(!cardinality_rep.has_weights());
    assert!(weighted_rep.has_weights());
}

#[test]
fn weight_lits_rep_literals_returns_prefix_up_to_size() {
    let lits = [
        WeightLiteral {
            lit: pos_lit(1),
            weight: 3,
        },
        WeightLiteral {
            lit: pos_lit(2),
            weight: 2,
        },
        WeightLiteral {
            lit: pos_lit(3),
            weight: 1,
        },
    ];
    let rep = WeightLitsRep {
        lits: &lits,
        size: 2,
        bound: 1,
        reach: 6,
    };

    assert_eq!(rep.literals(), &lits[..2]);
}
