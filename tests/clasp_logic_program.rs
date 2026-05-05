use rust_clasp::clasp::logic_program::{
    AspOptions, AtomSorting, BodyStats, ExtendedRuleMode, RuleStats, RuleStatsKey,
};
use rust_clasp::potassco::basic_types::{BodyType, HeadType};

#[test]
fn asp_options_defaults_match_upstream_layout() {
    let options = AspOptions::default();

    assert_eq!(options.er_mode, ExtendedRuleMode::Native);
    assert_eq!(options.iters, 5);
    assert_eq!(options.sort_atom, AtomSorting::Auto);
    assert!(!options.no_scc);
    assert!(!options.supp_mod);
    assert!(!options.df_order);
    assert!(!options.backprop);
    assert!(!options.old_map);
    assert!(!options.no_gamma);
}

#[test]
fn asp_options_mutators_match_header_semantics() {
    let mut options = AspOptions::default();

    options
        .iterations(AspOptions::MAX_EQ_ITERS + 9)
        .depth_first()
        .backpropagate()
        .no_scc()
        .disable_gamma()
        .ext(ExtendedRuleMode::TransformNhcf)
        .sort(AtomSorting::ArityNatural);

    assert_eq!(options.iters, AspOptions::MAX_EQ_ITERS);
    assert!(options.df_order);
    assert!(options.backprop);
    assert!(options.no_scc);
    assert!(options.no_gamma);
    assert_eq!(options.er_mode, ExtendedRuleMode::TransformNhcf);
    assert_eq!(options.sort_atom, AtomSorting::ArityNatural);

    options.no_eq();
    assert_eq!(options.iters, 0);
}

#[test]
fn body_stats_num_keys_matches_upstream_body_type_range() {
    assert_eq!(BodyStats::num_keys(), BodyType::Count as u32 + 1);
}

#[test]
fn body_stats_indexing_matches_upstream_slot_access() {
    let mut stats = BodyStats::default();
    stats[BodyType::Sum as u32] = 7;

    assert_eq!(stats[BodyType::Normal as u32], 0);
    assert_eq!(stats[BodyType::Sum as u32], 7);
}

#[test]
fn body_stats_sum_matches_upstream_accumulation() {
    let mut stats = BodyStats::default();
    stats[BodyType::Normal as u32] = 2;
    stats[BodyType::Sum as u32] = 3;
    stats[BodyType::Count as u32] = 5;

    assert_eq!(stats.sum(), 10);
}

#[test]
fn body_stats_to_str_matches_upstream_names() {
    assert_eq!(BodyStats::to_str(BodyType::Normal as u32), "Normal");
    assert_eq!(BodyStats::to_str(BodyType::Sum as u32), "Sum");
    assert_eq!(BodyStats::to_str(BodyType::Count as u32), "Count");
}

#[test]
fn body_stats_up_matches_upstream_counter_increment() {
    let mut stats = BodyStats::default();
    stats.up(BodyType::Count, 3);
    stats.up(BodyType::Count, 2);

    assert_eq!(stats[BodyType::Count as u32], 5);
}

#[test]
fn rule_stats_num_keys_matches_upstream_key_count() {
    assert_eq!(RuleStats::num_keys(), RuleStatsKey::KeyNum as u32);
    assert_eq!(RuleStatsKey::Normal as u32, HeadType::Disjunctive as u32);
    assert_eq!(RuleStatsKey::Choice as u32, HeadType::Choice as u32);
}

#[test]
fn rule_stats_indexing_matches_upstream_slot_access() {
    let mut stats = RuleStats::default();
    stats[RuleStatsKey::Heuristic as u32] = 4;

    assert_eq!(stats[RuleStatsKey::Normal as u32], 0);
    assert_eq!(stats[RuleStatsKey::Heuristic as u32], 4);
}

#[test]
fn rule_stats_sum_matches_upstream_accumulation() {
    let mut stats = RuleStats::default();
    stats[RuleStatsKey::Normal as u32] = 1;
    stats[RuleStatsKey::Minimize as u32] = 2;
    stats[RuleStatsKey::Acyc as u32] = 3;

    assert_eq!(stats.sum(), 6);
}

#[test]
fn rule_stats_to_str_matches_upstream_names() {
    assert_eq!(RuleStats::to_str(RuleStatsKey::Normal as u32), "Normal");
    assert_eq!(RuleStats::to_str(RuleStatsKey::Choice as u32), "Choice");
    assert_eq!(RuleStats::to_str(RuleStatsKey::Minimize as u32), "Minimize");
    assert_eq!(RuleStats::to_str(RuleStatsKey::Acyc as u32), "Acyc");
    assert_eq!(
        RuleStats::to_str(RuleStatsKey::Heuristic as u32),
        "Heuristic"
    );
    assert_eq!(RuleStats::to_str(RuleStats::num_keys()), "None");
}

#[test]
fn rule_stats_up_matches_upstream_counter_increment() {
    let mut stats = RuleStats::default();
    stats.up(RuleStatsKey::Choice, 2);
    stats.up(RuleStatsKey::Choice, 5);

    assert_eq!(stats[RuleStatsKey::Choice as u32], 7);
}
