use rust_clasp::clasp::literal::{VarType, neg_lit, pos_lit};
use rust_clasp::clasp::lookahead::{
    Lookahead, LookaheadParams, ScoreLook, ScoreLookMode, ScoreLookVarInfo, VarScore,
};

#[test]
fn var_score_tracks_tests_preferences_and_dependent_scores() {
    let positive = pos_lit(3);
    let negative = neg_lit(3);
    let mut score = VarScore::default();

    score.set_score(positive, 9);
    score.set_score(negative, VarScore::MAX_SCORE + 100);

    assert!(score.tested_lit(positive));
    assert!(score.tested_lit(negative));
    assert!(score.tested_any());
    assert!(score.tested_both());
    assert_eq!(score.p_val(), 9);
    assert_eq!(score.n_val(), VarScore::MAX_SCORE);
    assert!(score.pref_sign());

    score.set_dep_score(positive, 7);
    score.set_dep_score(positive, 11);
    score.set_dep_score(negative, 5);

    assert!(score.seen_lit(positive));
    assert!(score.seen_lit(negative));
    assert!(score.seen_any());
    assert_eq!(score.score(positive), 7);
    assert_eq!(score.score(negative), 5);
    assert_eq!(score.score_pair(), (7, 5));
}

#[test]
fn score_look_comparisons_and_clear_deps_match_upstream_helpers() {
    let mut score = ScoreLook {
        score: vec![VarScore::default(); 4],
        deps: vec![1, 2],
        best: 2,
        limit: 12,
        mode: ScoreLookMode::ScoreMax,
        ..ScoreLook::default()
    };
    score.score[1].set_score(pos_lit(1), 10);
    score.score[2].set_score(pos_lit(2), 8);
    score.score[2].set_score(neg_lit(2), 2);
    score.score[3].set_score(pos_lit(3), 8);
    score.score[3].set_score(neg_lit(3), 7);

    assert!(score.valid_var(3));
    assert!(!score.valid_var(4));
    assert!(score.greater(1, 2));
    assert!(score.greater_max(1, 9));
    assert!(!score.greater_max(2, 8));

    score.mode = ScoreLookMode::ScoreMaxMin;
    assert!(score.greater(3, 2));
    assert!(score.greater_max_min(3, 8, 2));
    assert!(!score.greater_max_min(2, 8, 2));

    score.clear_deps();

    assert!(score.deps.is_empty());
    assert_eq!(score.best, 0);
    assert_eq!(score.limit, u32::MAX);
    assert_eq!(score.score[1], VarScore::default());
    assert_eq!(score.score[2], VarScore::default());
    assert_ne!(score.score[3], VarScore::default());
}

#[test]
fn lookahead_params_and_constructor_preserve_header_semantics() {
    let params = LookaheadParams::new(VarType::Body)
        .add_imps(false)
        .nant(true)
        .limit(3);
    let body = Lookahead::new(params);
    let hybrid = Lookahead::new(LookaheadParams::default().lookahead(VarType::Hybrid));

    assert!(Lookahead::is_type(VarType::Atom as u32));
    assert!(Lookahead::is_type(VarType::Hybrid as u32));
    assert!(!Lookahead::is_type(0));
    assert!(!Lookahead::is_type(4));

    assert_eq!(body.priority(), Lookahead::PRIO);
    assert!(body.has_limit());
    assert_eq!(body.limit(), 3);
    assert!(!body.top_level_imps());
    assert_eq!(body.score.types, VarType::Body);
    assert_eq!(body.score.mode, ScoreLookMode::ScoreMaxMin);
    assert!(body.score.nant);

    assert!(!hybrid.has_limit());
    assert!(hybrid.top_level_imps());
    assert_eq!(hybrid.score.types, VarType::Hybrid);
    assert_eq!(hybrid.score.mode, ScoreLookMode::ScoreMax);
    assert!(!hybrid.score.nant);
}

#[test]
fn score_look_counts_nant_and_scores_dependencies_like_upstream_source() {
    let infos = [
        ScoreLookVarInfo::new(VarType::Atom, false),
        ScoreLookVarInfo::new(VarType::Atom, true),
        ScoreLookVarInfo::new(VarType::Body, false),
        ScoreLookVarInfo::new(VarType::Atom, true),
        ScoreLookVarInfo::new(VarType::Hybrid, false),
    ];
    let literals = [pos_lit(1), neg_lit(3), pos_lit(4)];
    let mut score = ScoreLook {
        score: vec![VarScore::default(); infos.len()],
        types: VarType::Atom,
        mode: ScoreLookMode::ScoreMax,
        add_deps: true,
        nant: true,
        ..ScoreLook::default()
    };

    assert_eq!(
        ScoreLook::count_nant_with(&literals, |var| infos[var as usize]),
        3
    );

    score.score_lits_with(&literals, |var| infos[var as usize]);

    assert_eq!(score.score[1].score(pos_lit(1)), 3);
    assert_eq!(score.score[3].score(neg_lit(3)), 3);
    assert_eq!(score.score[4].score(pos_lit(4)), 3);
    assert_eq!(score.deps, vec![1, 3, 4]);
    assert_eq!(score.best, 1);
    assert!(score.score[1].tested_lit(pos_lit(1)));
    assert!(score.score[1].seen_lit(pos_lit(1)));
    assert!(score.score[3].seen_lit(neg_lit(3)));
}

#[test]
fn score_look_respects_add_deps_and_max_min_best_updates() {
    let infos = [
        ScoreLookVarInfo::new(VarType::Atom, false),
        ScoreLookVarInfo::new(VarType::Atom, false),
        ScoreLookVarInfo::new(VarType::Atom, false),
    ];
    let literals = [neg_lit(1), pos_lit(1), pos_lit(2)];
    let mut score = ScoreLook {
        score: vec![VarScore::default(); infos.len()],
        types: VarType::Atom,
        mode: ScoreLookMode::ScoreMaxMin,
        add_deps: true,
        best: 2,
        ..ScoreLook::default()
    };
    score.score[1].set_score(pos_lit(1), 3);
    score.score[2].set_score(pos_lit(2), 2);
    score.score[2].set_score(neg_lit(2), 1);

    score.score_lits_with(&literals, |var| infos[var as usize]);

    assert_eq!(score.best, 1);
    assert_eq!(score.score[1].score_pair(), (3, 3));
    assert_eq!(score.deps, vec![1, 2]);

    let mut no_deps = ScoreLook {
        score: vec![VarScore::default(); infos.len()],
        add_deps: false,
        ..ScoreLook::default()
    };
    no_deps.score_lits_with(&[pos_lit(1)], |var| infos[var as usize]);
    assert!(no_deps.deps.is_empty());
    assert_eq!(no_deps.best, 0);
    assert!(no_deps.score[1].tested_lit(pos_lit(1)));
    assert!(!no_deps.score[1].seen_any());
}
