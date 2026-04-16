use rust_clasp::clasp::cli::clasp_cli_options::{opt_params, restart_schedule};
use rust_clasp::clasp::cli::clasp_options::{
    ConfigKey, format_config_key, format_opt_params, format_restart_schedule,
    format_sat_pre_params, parse_config_key, parse_opt_params, parse_restart_schedule,
    parse_sat_pre_params, set_opt_legacy,
};
use rust_clasp::clasp::solver_strategies::{OptParams, RestartKeep, RestartSchedule};
use rust_clasp::clasp::util::misc_types::MovingAvgType;

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
