use rust_clasp::clasp::literal::{WeightLiteral, pos_lit};
use rust_clasp::clasp::minimize_constraint::{LevelWeight, MinimizeMode, SharedMinimizeData};

fn negative_lower_sample() -> SharedMinimizeData {
    SharedMinimizeData::from_parts(
        vec![0, 0, 0],
        vec![
            LevelWeight::new(0, 1, true),
            LevelWeight::new(1, -1, true),
            LevelWeight::new(2, 1, false),
        ],
        vec![3, 2, 1],
        vec![
            WeightLiteral {
                lit: pos_lit(1),
                weight: 0,
            },
            WeightLiteral {
                lit: pos_lit(2),
                weight: 0,
            },
        ],
        MinimizeMode::Optimize,
    )
}

#[test]
fn negative_lower_initialization_matches_upstream() {
    let data = negative_lower_sample();

    assert_eq!(data.lower(0), 0);
    assert_eq!(data.lower(1), -2);
    assert_eq!(data.lower(2), 0);
}

#[test]
fn adjusted_optimum_matches_upstream_adjust_behavior() {
    let mut data = SharedMinimizeData::from_parts(
        vec![-2],
        Vec::new(),
        vec![0],
        vec![
            WeightLiteral {
                lit: pos_lit(1),
                weight: 1,
            },
            WeightLiteral {
                lit: pos_lit(2),
                weight: 1,
            },
        ],
        MinimizeMode::Optimize,
    );

    data.set_optimum(&[2]);
    assert_eq!(data.optimum(0), 0);

    data.set_optimum(&[0]);
    assert_eq!(data.optimum(0), -2);
}

#[test]
fn set_mode_rejects_bounds_below_current_lower_bound() {
    let mut data = negative_lower_sample();

    assert!(!data.set_mode(MinimizeMode::Enumerate, &[0, -3, 0]));
    assert!(data.set_mode(MinimizeMode::Enumerate, &[0, -2, 0]));
    assert_eq!(data.upper(1), -2);
    assert_eq!(data.optimum(1), SharedMinimizeData::max_bound());
}

#[test]
fn marking_optimal_freezes_future_optimum_updates_until_reset() {
    let mut data = SharedMinimizeData::from_parts(
        vec![-2],
        Vec::new(),
        vec![0],
        vec![WeightLiteral {
            lit: pos_lit(1),
            weight: 1,
        }],
        MinimizeMode::Optimize,
    );

    assert_eq!(data.generation(), 0);
    assert_eq!(data.set_optimum(&[2]), &[2]);
    assert_eq!(data.generation(), 1);
    data.mark_optimal();

    assert_eq!(data.set_optimum(&[10]), &[2]);
    assert_eq!(data.generation(), 1);
    assert_eq!(data.optimum(0), 0);

    data.reset_bounds();
    assert_eq!(data.generation(), 0);
    assert_eq!(data.set_optimum(&[0]), &[0]);
    assert_eq!(data.optimum(0), -2);
}
