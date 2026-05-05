use std::panic::{self, AssertUnwindSafe};

use rust_clasp::clasp::literal::VarType;
use rust_clasp::clasp::logic_program::{
    AspOptions, AtomSorting, BodyStats, ExtendedRuleMode, LpStats, RuleStats, RuleStatsKey,
};
use rust_clasp::clasp::logic_program_types::PrgNode;
use rust_clasp::clasp::statistics::{StatisticMap, StatisticObject};
use rust_clasp::potassco::basic_types::{BodyType, HeadType};
use rust_clasp::potassco::error::Error;

fn catch_error<F>(func: F) -> Error
where
    F: FnOnce(),
{
    let payload = panic::catch_unwind(AssertUnwindSafe(func)).expect_err("expected panic");
    *payload
        .downcast::<Error>()
        .expect("expected potassco error")
}

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

#[test]
fn lp_stats_eqs_accessors_match_upstream_slots() {
    let mut stats = LpStats::default();

    stats.inc_eqs(VarType::Atom);
    stats.inc_eqs(VarType::Body);
    stats.inc_eqs(VarType::Body);
    stats.inc_eqs(VarType::Hybrid);

    assert_eq!(stats.eqs_for(VarType::Atom), 1);
    assert_eq!(stats.eqs_for(VarType::Body), 2);
    assert_eq!(stats.eqs_for(VarType::Hybrid), 1);
    assert_eq!(stats.eqs(), 4);
}

#[test]
fn lp_stats_accu_matches_upstream_accumulation_and_scc_reset_branch() {
    let mut lhs = LpStats::default();
    lhs.atoms = 2;
    lhs.aux_atoms = 1;
    lhs.ufs_nodes = 3;
    lhs.disjunctions = [4, 5];
    lhs.sccs = 7;
    lhs.non_hcfs = 8;
    lhs.gammas = 9;
    lhs.bodies[0][BodyType::Normal as u32] = 1;
    lhs.bodies[1][BodyType::Count as u32] = 2;
    lhs.rules[0][RuleStatsKey::Normal as u32] = 3;
    lhs.rules[1][RuleStatsKey::Heuristic as u32] = 4;
    lhs.inc_eqs(VarType::Atom);

    let mut rhs = LpStats::default();
    rhs.atoms = 11;
    rhs.aux_atoms = 12;
    rhs.ufs_nodes = 13;
    rhs.disjunctions = [14, 15];
    rhs.sccs = 16;
    rhs.non_hcfs = 17;
    rhs.gammas = 18;
    rhs.bodies[0][BodyType::Normal as u32] = 19;
    rhs.bodies[1][BodyType::Count as u32] = 20;
    rhs.rules[0][RuleStatsKey::Normal as u32] = 21;
    rhs.rules[1][RuleStatsKey::Heuristic as u32] = 22;
    rhs.inc_eqs(VarType::Body);

    lhs.accu(&rhs);

    assert_eq!(lhs.atoms, 13);
    assert_eq!(lhs.aux_atoms, 13);
    assert_eq!(lhs.ufs_nodes, 16);
    assert_eq!(lhs.disjunctions, [18, 20]);
    assert_eq!(lhs.sccs, 23);
    assert_eq!(lhs.non_hcfs, 25);
    assert_eq!(lhs.gammas, 27);
    assert_eq!(lhs.bodies[0][BodyType::Normal as u32], 20);
    assert_eq!(lhs.bodies[1][BodyType::Count as u32], 22);
    assert_eq!(lhs.rules[0][RuleStatsKey::Normal as u32], 24);
    assert_eq!(lhs.rules[1][RuleStatsKey::Heuristic as u32], 26);
    assert_eq!(lhs.eqs_for(VarType::Atom), 1);
    assert_eq!(lhs.eqs_for(VarType::Body), 1);

    let mut reset = LpStats::default();
    reset.sccs = PrgNode::SCC_NOT_SET;
    reset.non_hcfs = 99;
    reset.gammas = 98;
    reset.accu(&rhs);
    assert_eq!(reset.sccs, rhs.sccs);
    assert_eq!(reset.non_hcfs, rhs.non_hcfs);
    assert_eq!(reset.gammas, rhs.gammas);
}

#[test]
fn lp_stats_statistic_map_matches_upstream_key_table_and_values() {
    let mut stats = LpStats::default();
    stats.atoms = 10;
    stats.aux_atoms = 11;
    stats.disjunctions = [12, 13];
    stats.sccs = 14;
    stats.non_hcfs = 15;
    stats.gammas = 16;
    stats.ufs_nodes = 17;
    stats.bodies[0][BodyType::Normal as u32] = 2;
    stats.bodies[0][BodyType::Sum as u32] = 3;
    stats.bodies[1][BodyType::Count as u32] = 4;
    stats.rules[0][RuleStatsKey::Normal as u32] = 5;
    stats.rules[0][RuleStatsKey::Heuristic as u32] = 6;
    stats.rules[1][RuleStatsKey::Choice as u32] = 7;
    stats.inc_eqs(VarType::Atom);
    stats.inc_eqs(VarType::Hybrid);

    let object = StatisticObject::map(&stats);

    assert_eq!(LpStats::size(), 30);
    assert_eq!(object.size(), LpStats::size());
    assert_eq!(LpStats::key(0), "atoms");
    assert_eq!(LpStats::key(LpStats::size() - 1), "eqs_other");
    assert_eq!(StatisticMap::key(&stats, 1), "atoms_aux");
    assert_eq!(object.at("atoms").value(), 10.0);
    assert_eq!(object.at("atoms_aux").value(), 11.0);
    assert_eq!(object.at("disjunctions").value(), 12.0);
    assert_eq!(object.at("disjunctions_non_hcf").value(), 13.0);
    assert_eq!(object.at("bodies").value(), 5.0);
    assert_eq!(object.at("sum_bodies").value(), 3.0);
    assert_eq!(object.at("count_bodies_tr").value(), 4.0);
    assert_eq!(object.at("rules").value(), 11.0);
    assert_eq!(object.at("rules_tr_choice").value(), 7.0);
    assert_eq!(object.at("eqs").value(), 2.0);
    assert_eq!(object.at("eqs_atom").value(), 1.0);
    assert_eq!(object.at("eqs_other").value(), 1.0);
}

#[test]
fn lp_stats_invalid_key_and_index_match_out_of_range_failures() {
    let stats = LpStats::default();

    let bad_index = catch_error(|| {
        let _ = LpStats::key(LpStats::size());
    });
    assert_eq!(
        bad_index,
        Error::OutOfRange(format!(
            "invalid LpStats index {} (size: 30)",
            LpStats::size()
        ))
    );

    let bad_key = catch_error(|| {
        let _ = stats.at("missing");
    });
    assert_eq!(
        bad_key,
        Error::OutOfRange("invalid LpStats key: missing".to_owned())
    );
}
