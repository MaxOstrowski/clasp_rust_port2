use rust_clasp::clasp::literal::{WeightLiteral, is_sentinel, lit_true, neg_lit, pos_lit};
use rust_clasp::clasp::minimize_constraint::{
    LevelWeight, MinimizeBuilder, MinimizeMode, SharedMinimizeData,
};
use rust_clasp::clasp::shared_context::SharedContext;

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

#[test]
fn minimize_builder_empty_multi_level_matches_upstream_sentinel_shape() {
    let mut ctx = SharedContext::default();
    let mut builder = MinimizeBuilder::new();

    builder.add_literal(
        0,
        WeightLiteral {
            lit: lit_true,
            weight: 1,
        },
    );
    builder.add_literal(
        1,
        WeightLiteral {
            lit: lit_true,
            weight: 1,
        },
    );

    let data = builder
        .build(&mut ctx)
        .expect("sentinel-only minimize data");
    assert!(is_sentinel(data.literals()[0].lit));
    assert_eq!(data.literals()[0].weight, 0);
    assert_eq!(data.weights.len(), 1);
}

#[test]
fn minimize_builder_one_level_lits_match_upstream_normalization() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();
    let d = ctx.add_var();
    let e = ctx.add_var();
    let mut builder = MinimizeBuilder::new();

    assert!(ctx.add_unary(pos_lit(c)));
    assert!(ctx.add_unary(neg_lit(e)));

    builder.add_literal(
        0,
        WeightLiteral {
            lit: pos_lit(a),
            weight: 1,
        },
    );
    builder.add_literal(
        0,
        WeightLiteral {
            lit: pos_lit(b),
            weight: 2,
        },
    );
    builder.add_literal(
        0,
        WeightLiteral {
            lit: pos_lit(c),
            weight: 1,
        },
    );
    builder.add_literal(
        0,
        WeightLiteral {
            lit: pos_lit(a),
            weight: 2,
        },
    );
    builder.add_literal(
        0,
        WeightLiteral {
            lit: pos_lit(d),
            weight: 1,
        },
    );
    builder.add_literal(
        0,
        WeightLiteral {
            lit: pos_lit(e),
            weight: 2,
        },
    );

    let data = builder.build(&mut ctx).expect("single-level minimize data");
    let lits = data.iter().copied().collect::<Vec<_>>();
    assert_eq!(data.num_rules(), 1);
    assert_eq!(lits.len(), 3);
    assert_eq!(data.adjust(0), 1);
    assert_eq!(
        lits,
        vec![
            WeightLiteral {
                lit: pos_lit(a),
                weight: 3,
            },
            WeightLiteral {
                lit: pos_lit(b),
                weight: 2,
            },
            WeightLiteral {
                lit: pos_lit(d),
                weight: 1,
            },
        ]
    );
    assert!(ctx.var_info(a).frozen());
    assert!(ctx.var_info(b).frozen());
    assert!(ctx.var_info(d).frozen());
}

#[test]
fn minimize_builder_sparse_compare_matches_upstream_level_weights() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();
    let mut builder = MinimizeBuilder::new();

    builder.add_literal(
        0,
        WeightLiteral {
            lit: pos_lit(b),
            weight: 1,
        },
    );
    builder.add_literal(
        1,
        WeightLiteral {
            lit: pos_lit(a),
            weight: 1,
        },
    );
    builder.add_literal(
        2,
        WeightLiteral {
            lit: pos_lit(c),
            weight: -1,
        },
    );
    builder.add_literal(
        2,
        WeightLiteral {
            lit: pos_lit(b),
            weight: -2,
        },
    );
    builder.add_literal(
        2,
        WeightLiteral {
            lit: pos_lit(a),
            weight: -2,
        },
    );

    let data = builder.build(&mut ctx).expect("sparse minimize data");
    let lits = data.iter().copied().collect::<Vec<_>>();

    assert_eq!(lits.len(), 3);
    assert_eq!(
        lits[0],
        WeightLiteral {
            lit: neg_lit(b),
            weight: 0
        }
    );
    assert_eq!(
        lits[1],
        WeightLiteral {
            lit: neg_lit(a),
            weight: 2
        }
    );
    assert_eq!(
        lits[2],
        WeightLiteral {
            lit: neg_lit(c),
            weight: 4
        }
    );

    assert_eq!(data.weights[0], LevelWeight::new(0, 2, true));
    assert_eq!(data.weights[1], LevelWeight::new(2, -1, false));
    assert_eq!(data.weights[2], LevelWeight::new(0, 2, true));
    assert_eq!(data.weights[3], LevelWeight::new(1, -1, false));
    assert_eq!(data.weights[4], LevelWeight::new(0, 1, false));

    assert_eq!(data.adjust(0), -5);
    assert_eq!(data.adjust(1), 1);
    assert_eq!(data.adjust(2), 1);
}

#[test]
fn minimize_builder_init_from_other_preserves_shared_data_shape() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let mut builder = MinimizeBuilder::new();

    builder.add_literal(
        1,
        WeightLiteral {
            lit: neg_lit(a),
            weight: 2,
        },
    );
    builder.add_literal(
        1,
        WeightLiteral {
            lit: neg_lit(a),
            weight: -3,
        },
    );
    builder.add_adjust(1, rust_clasp::clasp::literal::weight_min);
    builder.add_literal(
        0,
        WeightLiteral {
            lit: neg_lit(a),
            weight: 1,
        },
    );
    builder.add_literal(
        0,
        WeightLiteral {
            lit: pos_lit(a),
            weight: -4,
        },
    );
    builder.add_adjust(0, rust_clasp::clasp::literal::weight_max);

    let first = builder.build(&mut ctx).expect("original minimize data");
    builder.add_shared(&first);
    let second = builder.build(&mut ctx).expect("rebuilt minimize data");

    assert_eq!(first.num_rules(), second.num_rules());
    assert_eq!(first.adjust_slice(), second.adjust_slice());
    assert_eq!(first.weights, second.weights);
    assert_eq!(first.literals(), second.literals());
}
