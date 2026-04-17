//! Partial port of `original_clasp/tests/decision_heuristic_test.cpp`.

use rust_clasp::clasp::constraint::ConstraintType;
use rust_clasp::clasp::heuristics::{
    BERK_MAX_DECAY, BerkminConfig, BerkminHScore, BerkminOrder, DomScore, VmtfConfig, VmtfVarInfo,
    VsidsConfig, VsidsDynDecay, VsidsScore, add_other, init_decay, moms_score_from_counts,
    normalize_activity_scores,
};
use rust_clasp::clasp::literal::{neg_lit, pos_lit};
use rust_clasp::clasp::solver_strategies::{DomMod, HeuParams, Score, ScoreOther, VsidsDecay};

#[test]
fn moms_score_formula_matches_upstream() {
    assert_eq!(moms_score_from_counts(3, 4), ((3 * 4) << 10) + 7);
    assert_eq!(moms_score_from_counts(0, 5), 5);
}

#[test]
fn berkmin_hscore_tracks_occurrence_and_lazy_decay() {
    let mut score = BerkminHScore::new(2);
    score.inc_act(2, false, false);
    score.inc_act(2, false, true);
    assert_eq!(score.occ, 0);
    assert_eq!(score.act, 2);

    let decayed = score.decay(4, true);
    assert_eq!(decayed, 0);
    assert_eq!(score.dec, 4);
}

#[test]
fn berkmin_config_normalizes_upstream_defaults() {
    let params = HeuParams::default();
    let config = BerkminConfig::new(params);

    assert_eq!(config.max_berkmin, u32::MAX);
    assert_eq!(config.order.res_score, Score::ScoreMultiSet as u8);
    assert!(config.types.contains(ConstraintType::Static));
    assert!(config.types.contains(ConstraintType::Loop));

    let mut types = rust_clasp::clasp::constraint::TypeSet::new();
    add_other(&mut types, ScoreOther::OtherNo as u32);
    assert!(!types.contains(ConstraintType::Loop));
    assert!(!types.contains(ConstraintType::Other));
}

#[test]
fn berkmin_order_reset_rebases_scores() {
    let mut order = BerkminOrder {
        score: vec![BerkminHScore::default(); 3],
        decay: BERK_MAX_DECAY,
        huang: true,
        nant: false,
        res_score: Score::ScoreMin as u8,
    };
    order.score[1] = BerkminHScore {
        occ: 8,
        act: 16,
        dec: 1,
    };
    order.score[2] = BerkminHScore {
        occ: -4,
        act: 8,
        dec: 2,
    };

    order.reset_decay();

    assert_eq!(order.decay, 0);
    assert_eq!(order.score[1].dec, 0);
    assert_eq!(order.score[2].dec, 0);
}

#[test]
fn berkmin_order_inc_and_compare_follow_header_gates() {
    let mut order = BerkminOrder {
        score: vec![BerkminHScore::default(); 4],
        decay: 3,
        huang: false,
        nant: true,
        res_score: Score::ScoreMultiSet as u8,
    };

    order.inc(pos_lit(1), false);
    assert_eq!(order.score[1].act, 0);
    assert_eq!(order.occ(1), 0);

    order.inc(pos_lit(1), true);
    order.inc(pos_lit(1), true);
    order.inc_occ(neg_lit(2));
    order.inc(neg_lit(2), true);

    assert_eq!(order.occ(1), 2);
    assert_eq!(order.occ(2), -2);
    assert!(order.compare(1, 2) > 0);
}

#[test]
fn berkmin_and_decay_helpers_match_explicit_other_settings() {
    let params = HeuParams {
        param: 7,
        score: Score::ScoreSet as u32,
        other: ScoreOther::OtherAll as u32,
        moms: 0,
        ..HeuParams::default()
    };
    let config = BerkminConfig::new(params);

    assert_eq!(config.max_berkmin, 7);
    assert_eq!(config.order.res_score, Score::ScoreSet as u8);
    assert!(config.types.contains(ConstraintType::Loop));
    assert!(config.types.contains(ConstraintType::Other));
    assert!(!config.types.contains(ConstraintType::Static));

    assert!((init_decay(0) - 0.95).abs() < f64::EPSILON);
    assert!((init_decay(95) - 0.95).abs() < f64::EPSILON);
    assert!((init_decay(1000) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn vmtf_config_and_var_info_match_header_helpers() {
    let params = HeuParams {
        param: 1,
        score: Score::ScoreAuto as u32,
        other: ScoreOther::OtherAuto as u32,
        moms: 1,
        nant: 1,
        ..HeuParams::default()
    };
    let config = VmtfConfig::new(params);
    let mut info = VmtfVarInfo {
        prev: 3,
        next: 4,
        act: 64,
        occ: 0,
        decay: 1,
    };

    assert_eq!(config.n_move, 2);
    assert_eq!(config.score_type, Score::ScoreMin as u32);
    assert!(config.nant);
    assert!(config.types.contains(ConstraintType::Conflict));
    assert!(config.types.contains(ConstraintType::Static));
    assert!(info.in_list());
    assert_eq!(info.activity(3), 4);
}

#[test]
fn vmtf_config_handles_explicit_other_and_non_min_scores() {
    let params = HeuParams {
        param: 0,
        score: Score::ScoreSet as u32,
        other: ScoreOther::OtherAll as u32,
        moms: 0,
        ..HeuParams::default()
    };
    let config = VmtfConfig::new(params);

    assert_eq!(config.n_move, 8);
    assert_eq!(config.score_type, Score::ScoreSet as u32);
    assert!(config.types.contains(ConstraintType::Loop));
    assert!(config.types.contains(ConstraintType::Other));
    assert!(!config.types.contains(ConstraintType::Conflict));
    assert!(!config.types.contains(ConstraintType::Static));
}

#[test]
fn vsids_config_handles_dynamic_decay_and_flags() {
    let mut params = HeuParams {
        param: 95,
        score: Score::ScoreAuto as u32,
        other: ScoreOther::OtherAuto as u32,
        moms: 1,
        nant: 1,
        acids: 1,
        ..HeuParams::default()
    };
    params.set_decay(VsidsDecay {
        init: 80,
        bump: 3,
        freq: 20,
    });
    let config = VsidsConfig::new(params);
    let mut score = VsidsScore::new(2.5);

    assert!(config.types.contains(ConstraintType::Conflict));
    assert!(config.types.contains(ConstraintType::Static));
    assert!(config.acids);
    assert!(config.nant);
    assert_eq!(config.score_type, Score::ScoreMin as u32);
    assert_eq!(config.dyn_decay.bump, 3);
    assert_eq!(config.dyn_decay.freq, 20);
    assert_eq!(config.dyn_decay.next, 20);
    assert!((config.decay - (1.0 / init_decay(95))).abs() > f64::EPSILON);

    score.set(4.0);
    assert_eq!(score.get(), 4.0);
    assert_eq!(VsidsScore::apply_factor(&[score], 0, 3.5), 3.5);
}

#[test]
fn vsids_config_without_dynamic_decay_uses_plain_defaults() {
    let params = HeuParams {
        param: 0,
        score: Score::ScoreSet as u32,
        other: ScoreOther::OtherAll as u32,
        moms: 0,
        ..HeuParams::default()
    };
    let config = VsidsConfig::new(params);

    assert_eq!(config.score_type, Score::ScoreSet as u32);
    assert_eq!(config.dyn_decay, Default::default());
    assert!((config.decay - (1.0 / 0.95)).abs() < f64::EPSILON);
    assert!(!config.types.contains(ConstraintType::Conflict));
    assert!(config.types.contains(ConstraintType::Loop));
    assert!(config.types.contains(ConstraintType::Other));
}

#[test]
fn vsids_dynamic_decay_advances_like_upstream_conflict_step() {
    let mut dyn_decay = VsidsDynDecay {
        curr: 0.80,
        stop: 0.95,
        bump: 3,
        freq: 2,
        next: 2,
    };
    let mut decay = 1.0 / dyn_decay.curr;

    dyn_decay.advance(&mut decay);
    assert_eq!(dyn_decay.next, 1);
    assert!((decay - (1.0 / 0.80)).abs() < f64::EPSILON);

    dyn_decay.advance(&mut decay);
    assert_eq!(dyn_decay.next, 2);
    assert!((dyn_decay.curr - 0.83).abs() < 1e-12);
    assert!((decay - (1.0 / 0.83)).abs() < 1e-12);
}

#[test]
fn vsids_dynamic_decay_stops_rearming_once_limit_is_reached() {
    let mut dyn_decay = VsidsDynDecay {
        curr: 0.94,
        stop: 0.95,
        bump: 2,
        freq: 1,
        next: 1,
    };
    let mut decay = 1.0 / dyn_decay.curr;

    dyn_decay.advance(&mut decay);

    assert_eq!(dyn_decay.next, 0);
    assert!((dyn_decay.curr - 0.96).abs() < 1e-12);
    assert!((decay - (1.0 / 0.96)).abs() < 1e-12);
}

#[test]
fn normalize_activity_scores_scales_positive_vsids_values_only() {
    let mut scores = [
        VsidsScore::new(0.0),
        VsidsScore::new(5.0),
        VsidsScore::new(-2.0),
    ];
    let mut inc = 4.0;

    normalize_activity_scores(&mut scores, &mut inc);

    assert!((inc - 4.0e-100).abs() < f64::MIN_POSITIVE);
    assert!(scores[1].get() > 0.0);
    assert!((scores[1].get() - 5.0e-100).abs() < 1e-112);
    assert_eq!(scores[0].get(), 0.0);
    assert_eq!(scores[2].get(), -2.0);
}

#[test]
fn dom_score_orders_by_level_before_value_and_applies_factor() {
    let mut low = DomScore::new(5.0);
    let mut high = DomScore::new(1.0);
    low.level = 1;
    high.level = 2;
    high.factor = -3;
    high.set_dom(DomMod::ModLevel as u32);

    assert!(high > low);
    assert!(high.is_dom());
    assert_eq!(DomScore::apply_factor(&[low, high], 1, 2.0), -6.0);
}

#[test]
fn dom_score_breaks_same_level_ties_by_value() {
    let mut lhs = DomScore::new(2.0);
    let mut rhs = DomScore::new(5.0);
    lhs.level = 3;
    rhs.level = 3;

    assert!(rhs > lhs);
    assert_eq!(DomScore::apply_factor(&[lhs, rhs], 0, 4.0), 4.0);

    rhs.factor = 2;
    assert_eq!(DomScore::apply_factor(&[lhs, rhs], 1, 1.5), 3.0);
}

#[test]
fn normalize_activity_scores_preserves_dom_metadata() {
    let mut scores = [DomScore::new(3.0), DomScore::new(0.0)];
    let mut inc = 8.0;
    scores[0].level = 4;
    scores[0].factor = -2;
    scores[0].set_dom(DomMod::ModSPos as u32);
    scores[0].sign = true;
    scores[0].init = true;

    normalize_activity_scores(&mut scores, &mut inc);

    assert!((inc - 8.0e-100).abs() < f64::MIN_POSITIVE);
    assert!((scores[0].get() - 3.0e-100).abs() < 1e-112);
    assert_eq!(scores[0].level, 4);
    assert_eq!(scores[0].factor, -2);
    assert!(scores[0].is_dom());
    assert!(scores[0].sign);
    assert!(scores[0].init);
    assert_eq!(scores[1].get(), 0.0);
}
