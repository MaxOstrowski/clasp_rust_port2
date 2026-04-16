use rust_clasp::clasp::cli::clasp_cli_options::{
    CliEnum, CliOptionGroup, HeuristicType, MinimizeMode, ProjectMode, VarType, asp_logic_program,
    context_params, default_unfounded_check, distributor_policy, enum_map, from_chars, heu_params,
    opt_params, option_catalog, option_paths, parse_exact, reduce_strategy, restart_params,
    restart_schedule, solve_options, solver_params, solver_strategies, to_chars,
};
use rust_clasp::clasp::util::misc_types::MovingAvgType;
use std::collections::HashSet;

fn assert_enum_cases<E>(cases: &[(&str, E)])
where
    E: CliEnum + Copy + Eq + core::fmt::Debug,
{
    let entries = enum_map::<E>();
    assert_eq!(entries.len(), cases.len());
    for (entry, (key, value)) in entries.iter().zip(cases.iter().copied()) {
        assert_eq!(entry.key, key);
        assert_eq!(entry.value, value);
        let parsed = parse_exact::<E>(key).unwrap();
        assert_eq!(parsed, value);

        let upper = key.to_ascii_uppercase();
        let parsed_upper = parse_exact::<E>(&upper).unwrap();
        assert_eq!(parsed_upper, value);

        let with_tail = format!("{key},rest");
        let (prefixed, consumed) = from_chars::<E>(&with_tail).unwrap();
        assert_eq!(prefixed, value);
        assert_eq!(consumed, key.len());
    }

    let mut seen = Vec::<E>::new();
    for (key, value) in cases.iter().copied() {
        if seen.contains(&value) {
            continue;
        }
        seen.push(value);
        let mut out = String::new();
        to_chars(&mut out, value);
        assert_eq!(out, key);
    }
}

#[test]
fn context_and_solver_option_enums_roundtrip() {
    assert_enum_cases(&[
        ("no", context_params::ShareMode::ShareNo),
        ("all", context_params::ShareMode::ShareAll),
        ("auto", context_params::ShareMode::ShareAuto),
        ("problem", context_params::ShareMode::ShareProblem),
        ("learnt", context_params::ShareMode::ShareLearnt),
    ]);
    assert_enum_cases(&[
        ("no", context_params::ShortSimpMode::SimpNo),
        ("learnt", context_params::ShortSimpMode::SimpLearnt),
        ("all", context_params::ShortSimpMode::SimpAll),
    ]);
    assert_enum_cases(&[
        ("atom", VarType::Atom),
        ("body", VarType::Body),
        ("hybrid", VarType::Hybrid),
    ]);
    assert_enum_cases(&[
        ("berkmin", HeuristicType::Berkmin),
        ("vmtf", HeuristicType::Vmtf),
        ("vsids", HeuristicType::Vsids),
        ("domain", HeuristicType::Domain),
        ("unit", HeuristicType::Unit),
        ("auto", HeuristicType::Def),
        ("none", HeuristicType::None),
    ]);
    assert_enum_cases(&[
        ("auto", heu_params::Score::ScoreAuto),
        ("min", heu_params::Score::ScoreMin),
        ("set", heu_params::Score::ScoreSet),
        ("multiset", heu_params::Score::ScoreMultiSet),
    ]);
    assert_enum_cases(&[
        ("auto", heu_params::ScoreOther::OtherAuto),
        ("no", heu_params::ScoreOther::OtherNo),
        ("loop", heu_params::ScoreOther::OtherLoop),
        ("all", heu_params::ScoreOther::OtherAll),
    ]);
    assert_enum_cases(&[
        ("asp", solver_strategies::SignHeu::SignAtom),
        ("pos", solver_strategies::SignHeu::SignPos),
        ("neg", solver_strategies::SignHeu::SignNeg),
        ("rnd", solver_strategies::SignHeu::SignRnd),
    ]);
    assert_enum_cases(&[
        ("level", heu_params::DomMod::ModLevel),
        ("pos", heu_params::DomMod::ModSPos),
        ("true", heu_params::DomMod::ModTrue),
        ("neg", heu_params::DomMod::ModSNeg),
        ("false", heu_params::DomMod::ModFalse),
        ("init", heu_params::DomMod::ModInit),
        ("factor", heu_params::DomMod::ModFactor),
    ]);
    assert_enum_cases(&[
        ("all", heu_params::DomPref::PrefAtom),
        ("scc", heu_params::DomPref::PrefScc),
        ("hcc", heu_params::DomPref::PrefHcc),
        ("disj", heu_params::DomPref::PrefDisj),
        ("opt", heu_params::DomPref::PrefMin),
        ("show", heu_params::DomPref::PrefShow),
    ]);
    assert_enum_cases(&[
        ("rnd", solver_strategies::WatchInit::WatchRand),
        ("first", solver_strategies::WatchInit::WatchFirst),
        ("least", solver_strategies::WatchInit::WatchLeast),
    ]);
    assert_enum_cases(&[
        (
            "propagate",
            solver_strategies::UpdateMode::UpdateOnPropagate,
        ),
        ("conflict", solver_strategies::UpdateMode::UpdateOnConflict),
    ]);
    assert_enum_cases(&[
        ("varScores", solver_params::Forget::ForgetHeuristic),
        ("signs", solver_params::Forget::ForgetSigns),
        ("lemmaScores", solver_params::Forget::ForgetActivities),
        ("lemmas", solver_params::Forget::ForgetLearnts),
    ]);
    assert_enum_cases(&[
        ("local", solver_strategies::CCMinType::CcLocal),
        ("recursive", solver_strategies::CCMinType::CcRecursive),
    ]);
    assert_enum_cases(&[
        ("all", solver_strategies::CCMinAntes::AllAntes),
        ("short", solver_strategies::CCMinAntes::ShortAntes),
        ("binary", solver_strategies::CCMinAntes::BinaryAntes),
    ]);
    assert_enum_cases(&[
        ("less", solver_strategies::LbdMode::LbdUpdatedLess),
        ("glucose", solver_strategies::LbdMode::LbdUpdateGlucose),
        ("pseudo", solver_strategies::LbdMode::LbdUpdatePseudo),
    ]);
    assert_enum_cases(&[
        ("no", solver_strategies::CCRepMode::CcNoReplace),
        ("decisionSeq", solver_strategies::CCRepMode::CcRepDecision),
        ("allUIP", solver_strategies::CCRepMode::CcRepUip),
        ("dynamic", solver_strategies::CCRepMode::CcRepDynamic),
    ]);
    assert_enum_cases(&[
        (
            "common",
            default_unfounded_check::ReasonStrategy::CommonReason,
        ),
        (
            "shared",
            default_unfounded_check::ReasonStrategy::SharedReason,
        ),
        (
            "distinct",
            default_unfounded_check::ReasonStrategy::DistinctReason,
        ),
        ("no", default_unfounded_check::ReasonStrategy::OnlyReason),
    ]);
}

#[test]
fn optimization_restart_and_parallel_enums_roundtrip() {
    assert_enum_cases(&[
        ("bb", opt_params::Type::TypeBb),
        ("usc", opt_params::Type::TypeUsc),
    ]);
    assert_enum_cases(&[
        ("lin", opt_params::BBAlgo::BbLin),
        ("hier", opt_params::BBAlgo::BbHier),
        ("inc", opt_params::BBAlgo::BbInc),
        ("dec", opt_params::BBAlgo::BbDec),
    ]);
    assert_enum_cases(&[
        ("oll", opt_params::UscAlgo::UscOll),
        ("one", opt_params::UscAlgo::UscOne),
        ("k", opt_params::UscAlgo::UscK),
        ("pmres", opt_params::UscAlgo::UscPmr),
    ]);
    assert_enum_cases(&[
        ("disjoint", opt_params::UscOption::UscDisjoint),
        ("succinct", opt_params::UscOption::UscSuccinct),
        ("stratify", opt_params::UscOption::UscStratify),
    ]);
    assert_enum_cases(&[
        ("lin", opt_params::UscTrim::UscTrimLin),
        ("rgs", opt_params::UscTrim::UscTrimRgs),
        ("min", opt_params::UscTrim::UscTrimMin),
        ("exp", opt_params::UscTrim::UscTrimExp),
        ("inv", opt_params::UscTrim::UscTrimInv),
        ("bin", opt_params::UscTrim::UscTrimBin),
    ]);
    assert_enum_cases(&[
        ("sign", opt_params::Heuristic::HeuSign),
        ("model", opt_params::Heuristic::HeuModel),
    ]);
    assert_enum_cases(&[
        ("n", restart_schedule::Keep::KeepNever),
        ("r", restart_schedule::Keep::KeepRestart),
        ("b", restart_schedule::Keep::KeepBlock),
        ("br", restart_schedule::Keep::KeepAlways),
        ("rb", restart_schedule::Keep::KeepAlways),
    ]);
    assert_enum_cases(&[
        ("no", restart_params::SeqUpdate::SeqContinue),
        ("repeat", restart_params::SeqUpdate::SeqRepeat),
        ("disable", restart_params::SeqUpdate::SeqDisable),
    ]);
    assert_enum_cases(&[
        ("d", MovingAvgType::AvgSma),
        ("e", MovingAvgType::AvgEma),
        ("l", MovingAvgType::AvgEmaLog),
        ("es", MovingAvgType::AvgEmaSmooth),
        ("ls", MovingAvgType::AvgEmaLogSmooth),
    ]);
    assert_enum_cases(&[
        ("basic", reduce_strategy::Algorithm::ReduceLinear),
        ("sort", reduce_strategy::Algorithm::ReduceStable),
        ("ipSort", reduce_strategy::Algorithm::ReduceSort),
        ("ipHeap", reduce_strategy::Algorithm::ReduceHeap),
    ]);
    assert_enum_cases(&[
        ("activity", reduce_strategy::Score::ScoreAct),
        ("lbd", reduce_strategy::Score::ScoreLbd),
        ("mixed", reduce_strategy::Score::ScoreBoth),
    ]);
    assert_enum_cases(&[
        ("compete", solve_options::algorithm::SearchMode::ModeCompete),
        ("split", solve_options::algorithm::SearchMode::ModeSplit),
    ]);
    assert_enum_cases(&[
        ("global", solve_options::distribution::Mode::ModeGlobal),
        ("local", solve_options::distribution::Mode::ModeLocal),
    ]);
    assert_enum_cases(&[
        ("all", distributor_policy::Types::All),
        ("short", distributor_policy::Types::Implicit),
        ("conflict", distributor_policy::Types::Conflict),
        ("loop", distributor_policy::Types::Loop),
    ]);
    assert_enum_cases(&[
        ("all", solve_options::integration::Filter::FilterNo),
        ("gp", solve_options::integration::Filter::FilterGp),
        ("unsat", solve_options::integration::Filter::FilterSat),
        (
            "active",
            solve_options::integration::Filter::FilterHeuristic,
        ),
    ]);
    assert_enum_cases(&[
        ("all", solve_options::integration::Topology::TopoAll),
        ("ring", solve_options::integration::Topology::TopoRing),
        ("cube", solve_options::integration::Topology::TopoCube),
        ("cubex", solve_options::integration::Topology::TopoCubex),
    ]);
}

#[test]
fn asp_enumeration_projection_and_minimization_enums_roundtrip() {
    assert_enum_cases(&[
        ("no", asp_logic_program::ExtendedRuleMode::ModeNative),
        ("all", asp_logic_program::ExtendedRuleMode::ModeTransform),
        (
            "choice",
            asp_logic_program::ExtendedRuleMode::ModeTransformChoice,
        ),
        (
            "card",
            asp_logic_program::ExtendedRuleMode::ModeTransformCard,
        ),
        (
            "weight",
            asp_logic_program::ExtendedRuleMode::ModeTransformWeight,
        ),
        ("scc", asp_logic_program::ExtendedRuleMode::ModeTransformScc),
        (
            "integ",
            asp_logic_program::ExtendedRuleMode::ModeTransformInteg,
        ),
        (
            "dynamic",
            asp_logic_program::ExtendedRuleMode::ModeTransformDynamic,
        ),
    ]);
    assert_enum_cases(&[
        ("no", asp_logic_program::AtomSorting::SortNo),
        ("auto", asp_logic_program::AtomSorting::SortAuto),
        ("number", asp_logic_program::AtomSorting::SortNumber),
        ("name", asp_logic_program::AtomSorting::SortName),
        ("natural", asp_logic_program::AtomSorting::SortNatural),
        ("arity", asp_logic_program::AtomSorting::SortArity),
        ("full", asp_logic_program::AtomSorting::SortArityNatural),
    ]);
    assert_enum_cases(&[
        ("bt", solve_options::EnumType::EnumBt),
        ("record", solve_options::EnumType::EnumRecord),
        ("domRec", solve_options::EnumType::EnumDomRecord),
        ("brave", solve_options::EnumType::EnumBrave),
        ("cautious", solve_options::EnumType::EnumCautious),
        ("query", solve_options::EnumType::EnumQuery),
        ("auto", solve_options::EnumType::EnumAuto),
        ("user", solve_options::EnumType::EnumUser),
    ]);
    assert_enum_cases(&[
        ("auto", ProjectMode::Implicit),
        ("show", ProjectMode::Output),
        ("project", ProjectMode::Project),
    ]);
    assert_enum_cases(&[
        ("opt", MinimizeMode::Optimize),
        ("enum", MinimizeMode::Enumerate),
        ("optN", MinimizeMode::EnumOpt),
        ("ignore", MinimizeMode::Ignore),
    ]);
}

#[test]
fn unmapped_values_do_not_serialize_and_are_not_parsed() {
    let mut out = String::new();
    to_chars(&mut out, HeuristicType::User);
    assert!(out.is_empty());

    out.clear();
    to_chars(
        &mut out,
        asp_logic_program::ExtendedRuleMode::ModeTransformNhcf,
    );
    assert!(out.is_empty());

    out.clear();
    to_chars(&mut out, solve_options::EnumType::EnumConsequences);
    assert!(out.is_empty());

    assert!(parse_exact::<HeuristicType>("user").is_err());
    assert!(parse_exact::<asp_logic_program::ExtendedRuleMode>("nhcf").is_err());
    assert!(parse_exact::<solve_options::EnumType>("consequences").is_err());
}

fn group_keys(group: CliOptionGroup) -> Vec<&'static str> {
    option_catalog()
        .iter()
        .filter(|entry| entry.group == group)
        .map(|entry| entry.key)
        .collect()
}

#[test]
fn option_catalog_matches_upstream_groups() {
    assert_eq!(option_catalog().len(), 74);

    assert_eq!(
        group_keys(CliOptionGroup::Context),
        vec!["share", "learn_explicit", "short_simp_mode", "sat_prepro"]
    );
    assert_eq!(
        group_keys(CliOptionGroup::Global),
        vec!["stats", "parse_ext", "parse_maxsat"]
    );
    assert_eq!(
        group_keys(CliOptionGroup::Solver),
        vec![
            "opt_strategy",
            "opt_usc_shrink",
            "opt_heuristic",
            "restart_on_model",
            "lookahead",
            "heuristic",
            "init_moms",
            "score_res",
            "score_other",
            "sign_def",
            "sign_fix",
            "berk_huang",
            "vsids_acids",
            "vsids_progress",
            "nant",
            "dom_mod",
            "save_progress",
            "init_watches",
            "update_mode",
            "acyc_prop",
            "seed",
            "no_lookback",
            "forget_on_step",
            "strengthen",
            "otfs",
            "update_lbd",
            "update_act",
            "reverse_arcs",
            "contraction",
            "loops",
        ]
    );
    assert_eq!(
        group_keys(CliOptionGroup::Search),
        vec![
            "partial_check",
            "sign_def_disj",
            "rand_freq",
            "rand_prob",
            "restarts",
            "reset_restarts",
            "local_restarts",
            "counter_restarts",
            "block_restarts",
            "shuffle",
            "deletion",
            "del_grow",
            "del_cfl",
            "del_init",
            "del_estimate",
            "del_max",
            "del_glue",
            "del_on_restart",
        ]
    );
    assert_eq!(
        group_keys(CliOptionGroup::Asp),
        vec![
            "trans_ext",
            "eq",
            "sort_atoms",
            "backprop",
            "supp_models",
            "no_ufs_check",
            "no_gamma",
            "eq_dfs",
            "dlp_old_map",
        ]
    );
    assert_eq!(
        group_keys(CliOptionGroup::Solve),
        vec![
            "solve_limit",
            "parallel_mode",
            "global_restarts",
            "distribute",
            "integrate",
            "enum_mode",
            "project",
            "models",
            "opt_mode",
            "opt_stop",
        ]
    );
}

#[test]
fn option_paths_match_upstream_leaf_enumeration() {
    let keys = option_paths();
    let unique: HashSet<_> = keys.iter().collect();
    assert_eq!(keys.len(), unique.len());
    assert_eq!(keys.len(), 147);
    assert_eq!(keys[0], "configuration");
    assert_eq!(keys[1], "tester.configuration");

    for entry in option_catalog() {
        assert!(keys.contains(&entry.path()));
        match entry.group {
            CliOptionGroup::Global => {
                assert!(entry.tester_path().is_none());
            }
            _ => {
                assert!(keys.contains(&entry.tester_path().unwrap()));
            }
        }
    }

    assert!(!keys.contains(&String::from("tester.stats")));
    assert!(!keys.contains(&String::from("tester.parse_ext")));
    assert!(!keys.contains(&String::from("tester.parse_maxsat")));

    let sample_cli_names: Vec<_> = option_catalog()
        .iter()
        .filter(|entry| {
            matches!(
                entry.key,
                "learn_explicit"
                    | "short_simp_mode"
                    | "restart_on_model"
                    | "counter_restarts"
                    | "sort_atoms"
                    | "opt_mode"
            )
        })
        .map(|entry| entry.cli_name())
        .collect();
    assert_eq!(
        sample_cli_names,
        vec![
            "learn-explicit",
            "short-simp-mode",
            "restart-on-model",
            "counter-restarts",
            "sort-atoms",
            "opt-mode",
        ]
    );
}
