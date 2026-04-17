use rust_clasp::clasp::claspfwd::ProblemType;
use rust_clasp::clasp::cli::clasp_cli_options::{opt_params, restart_schedule};
use rust_clasp::clasp::cli::clasp_options::{
    ClaspCliConfig, ConfigKey, format_config_key, format_opt_params, format_restart_schedule,
    format_sat_pre_params, get_config, get_config_key, get_defaults, parse_config_key,
    parse_opt_params, parse_restart_schedule, parse_sat_pre_params, set_opt_legacy,
};
use rust_clasp::clasp::solver_strategies::{OptParams, RestartKeep, RestartSchedule};
use rust_clasp::clasp::util::misc_types::MovingAvgType;
use std::collections::HashSet;

fn collect_configs(
    mut iter: rust_clasp::clasp::cli::clasp_options::ConfigIter,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    while iter.valid() {
        out.push((
            iter.name().to_owned(),
            iter.base().to_owned(),
            iter.args().to_owned(),
        ));
        iter.next();
    }
    out
}

#[test]
fn config_key_roundtrips_known_presets() {
    let cases = [
        ("auto", ConfigKey::Default),
        ("frumpy", ConfigKey::Frumpy),
        ("jumpy", ConfigKey::Jumpy),
        ("tweety", ConfigKey::Tweety),
        ("handy", ConfigKey::Handy),
        ("crafty", ConfigKey::Crafty),
        ("trendy", ConfigKey::Trendy),
        ("tester", ConfigKey::Tester),
        ("many", ConfigKey::Many),
    ];
    for (text, key) in cases {
        assert_eq!(parse_config_key(text).unwrap(), key);
        assert_eq!(parse_config_key(&text.to_ascii_uppercase()).unwrap(), key);
        assert_eq!(format_config_key(key), text);
    }
    assert_eq!(parse_config_key("s6").unwrap(), ConfigKey::S6);
    assert_eq!(parse_config_key("nolearn").unwrap(), ConfigKey::Nolearn);
}

#[test]
fn clasp_options_exposes_upstream_config_helpers() {
    assert_eq!(get_defaults(ProblemType::Asp), "--configuration=tweety");
    assert_eq!(get_defaults(ProblemType::Sat), "--configuration=trendy");
    assert_eq!(get_defaults(ProblemType::Pb), "--configuration=trendy");

    assert_eq!(get_config_key("auto"), 0);
    assert_eq!(get_config_key("trendy"), 2);
    assert_eq!(get_config_key("S13"), 15);
    assert_eq!(get_config_key("missing"), -1);

    let trendy = collect_configs(get_config(ConfigKey::Trendy));
    assert_eq!(trendy, vec![(
        "[trendy]".to_owned(),
        String::new(),
        "--sat-p=2,iter=20,occ=25,time=240 --trans-ext=dynamic --heuristic=Vsids --restarts=D,100,0.7 --deletion=basic,50 --del-init=3.0,500,19500 --del-grow=1.1,20.0,x,100,1.5 --del-cfl=+,10000,2000 --del-glue=2 --strengthen=recursive --update-lbd=less --otfs=2 --save-p=75 --counter-restarts=3,1023 --reverse-arcs=2 --contraction=250 --loops=common".to_owned(),
    )]);

    let many = collect_configs(get_config(ConfigKey::Many));
    assert!(
        many.iter()
            .any(|(_, _, args)| args.contains("--opt-heu=sign --opt-strat=usc,disjoint"))
    );
}

#[test]
fn sat_pre_params_match_upstream_formats() {
    let disabled = parse_sat_pre_params("no").unwrap();
    assert_eq!(disabled.type_, 0);
    assert_eq!(disabled.lim_clause, 4000);
    assert_eq!(format_sat_pre_params(&disabled), "no");

    let keyed = parse_sat_pre_params("2,iter=40,occ=50,time=300").unwrap();
    assert_eq!(keyed.type_, 2);
    assert_eq!(keyed.lim_iters, 40);
    assert_eq!(keyed.lim_occ, 50);
    assert_eq!(keyed.lim_time, 300);
    assert_eq!(keyed.lim_clause, 4000);
    assert_eq!(
        format_sat_pre_params(&keyed),
        "2,iter=40,occ=50,time=300,size=4000"
    );

    let positional = parse_sat_pre_params("2,40,50,300").unwrap();
    assert_eq!(positional.lim_iters, 40);
    assert_eq!(positional.lim_occ, 50);
    assert_eq!(positional.lim_time, 300);
    assert_eq!(positional.lim_clause, 4000);

    assert!(parse_sat_pre_params("4").is_err());
    assert!(parse_sat_pre_params("2,iter=foo").is_err());
}

#[test]
fn opt_params_match_upstream_examples_and_legacy_modes() {
    let default_bb = parse_opt_params("bb").unwrap();
    assert_eq!(format_opt_params(&default_bb), "bb,lin");

    let bb_inc = parse_opt_params("bb,INC").unwrap();
    assert_eq!(bb_inc.type_, opt_params::Type::TypeBb as u32);
    assert_eq!(bb_inc.algo, opt_params::BBAlgo::BbInc as u32);
    assert_eq!(format_opt_params(&bb_inc), "bb,inc");

    let usc = parse_opt_params("usc").unwrap();
    assert_eq!(format_opt_params(&usc), "usc,oll");

    let usc_k = parse_opt_params("usc,k,4").unwrap();
    assert_eq!(usc_k.type_, opt_params::Type::TypeUsc as u32);
    assert_eq!(usc_k.algo, opt_params::UscAlgo::UscK as u32);
    assert_eq!(usc_k.k_lim, 4);
    assert_eq!(format_opt_params(&usc_k), "usc,k,4");

    let usc_opts = parse_opt_params("usc,oll,stratify,disjoint").unwrap();
    assert_eq!(
        usc_opts.opts,
        (opt_params::UscOption::UscDisjoint as u32) | (opt_params::UscOption::UscStratify as u32)
    );
    assert_eq!(format_opt_params(&usc_opts), "usc,oll,disjoint,stratify");

    let legacy_hier = parse_opt_params("1").unwrap();
    assert_eq!(format_opt_params(&legacy_hier), "bb,hier");

    let legacy_usc = parse_opt_params("5").unwrap();
    assert_eq!(format_opt_params(&legacy_usc), "usc,oll,disjoint");

    let legacy_usc_pmr = parse_opt_params("usc,15").unwrap();
    assert_eq!(
        format_opt_params(&legacy_usc_pmr),
        "usc,pmres,disjoint,succinct,stratify"
    );

    let mut legacy = OptParams::default();
    assert!(set_opt_legacy(&mut legacy, 7));
    assert_eq!(format_opt_params(&legacy), "usc,oll,disjoint,succinct");

    assert!(parse_opt_params("usc,foo").is_err());
    assert!(parse_opt_params("usc,oll,1,2").is_err());
    assert!(parse_opt_params("20").is_err());
}

#[test]
fn restart_schedule_matches_upstream_serialization() {
    let static_geom = parse_restart_schedule("x,100,1.5").unwrap();
    assert!(!static_geom.is_dynamic());
    assert_eq!(format_restart_schedule(&static_geom), "x,100,1.5");

    let dynamic = parse_restart_schedule("D,100,0.7").unwrap();
    assert!(dynamic.is_dynamic());
    assert_eq!(dynamic.base, 100);
    assert_eq!(dynamic.k(), 0.7);
    assert_eq!(dynamic.fast_avg(), MovingAvgType::AvgSma);
    assert_eq!(dynamic.slow_avg(), MovingAvgType::AvgSma);
    assert_eq!(format_restart_schedule(&dynamic), "d,100,0.7");

    let detailed = parse_restart_schedule("d,100,0.7,15,e,br,ls,32").unwrap();
    assert_eq!(detailed.lbd_lim(), 15);
    assert_eq!(detailed.fast_avg(), MovingAvgType::AvgEma);
    assert_eq!(detailed.keep_avg(), RestartKeep::Always);
    assert_eq!(detailed.slow_avg(), MovingAvgType::AvgEmaLogSmooth);
    assert_eq!(detailed.slow_win(), 32);
    assert_eq!(
        format_restart_schedule(&detailed),
        "d,100,0.7,15,e,br,ls,32"
    );

    let from_manual = RestartSchedule::dynamic(
        128,
        1.2,
        10,
        MovingAvgType::AvgEma,
        RestartKeep::Block,
        MovingAvgType::AvgEmaLog,
        20,
    );
    assert_eq!(
        format_restart_schedule(&from_manual),
        "d,128,1.2,10,e,b,l,20"
    );

    assert!(parse_restart_schedule("d,0,0.7").is_err());
    assert!(parse_restart_schedule("d,100,0").is_err());
    assert!(parse_restart_schedule("z,100").is_err());
    let _ = restart_schedule::Keep::KeepNever;
}

#[test]
fn clasp_cli_config_navigates_root_and_solver_keys() {
    let config = ClaspCliConfig::default();

    let root_children: HashSet<_> = (0..128)
        .map(|index| config.get_subkey(ClaspCliConfig::KEY_ROOT, index))
        .filter(|name| !name.is_empty())
        .collect();
    assert!(root_children.contains("configuration"));
    assert!(root_children.contains("tester"));
    assert!(root_children.contains("solver"));
    assert!(root_children.contains("asp"));
    assert!(root_children.contains("solve"));
    assert!(root_children.contains("share"));
    assert!(root_children.contains("stats"));

    let solver = config.get_key(ClaspCliConfig::KEY_ROOT, "solver");
    assert_eq!(solver, ClaspCliConfig::KEY_SOLVER);

    let lookahead = config.get_key(ClaspCliConfig::KEY_ROOT, "solver.lookahead");
    assert_ne!(lookahead, ClaspCliConfig::KEY_INVALID);
    assert!(ClaspCliConfig::is_leaf_key(lookahead));
    assert_eq!(
        lookahead,
        config.get_key(ClaspCliConfig::KEY_SOLVER, "lookahead")
    );

    let loops = config.get_key(ClaspCliConfig::KEY_ROOT, "solver.1.loops");
    assert_ne!(loops, ClaspCliConfig::KEY_INVALID);
    assert_eq!(loops, config.get_key(ClaspCliConfig::KEY_SOLVER, "loops"));
    assert_eq!(
        config.get_key(ClaspCliConfig::KEY_ROOT, "solver.2.loops"),
        ClaspCliConfig::KEY_INVALID
    );

    let solver_info = config.get_key_info(ClaspCliConfig::KEY_SOLVER).unwrap();
    assert_eq!(solver_info.array_len, 2);
    assert!(solver_info.subkey_count > 10);
    assert_eq!(solver_info.value_count, -1);

    let lookahead_info = config.get_key_info(lookahead).unwrap();
    assert_eq!(lookahead_info.array_len, -1);
    assert_eq!(lookahead_info.subkey_count, 0);
    assert_eq!(lookahead_info.value_count, 1);
}

#[test]
fn clasp_cli_config_rejects_unported_tester_subtrees() {
    let config = ClaspCliConfig::default();

    assert_eq!(
        config.get_key(ClaspCliConfig::KEY_ROOT, "tester.configuration"),
        config.get_key(ClaspCliConfig::KEY_TESTER, "configuration")
    );
    assert_eq!(
        config.get_key(ClaspCliConfig::KEY_ROOT, "tester.asp.trans_ext"),
        ClaspCliConfig::KEY_INVALID
    );
    assert_eq!(
        config.get_key(ClaspCliConfig::KEY_ROOT, "tester.solve.enum_mode"),
        ClaspCliConfig::KEY_INVALID
    );
    assert_eq!(
        config.get_arr_key(ClaspCliConfig::KEY_ROOT, 0),
        ClaspCliConfig::KEY_INVALID
    );
}
