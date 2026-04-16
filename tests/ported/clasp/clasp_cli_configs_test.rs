use rust_clasp::clasp::claspfwd::ProblemType;
use rust_clasp::clasp::cli::clasp_cli_configs::{
    AUX_CONFIGS, ConfigKey, DEFAULT_CONFIGS, config_entry, get_config, get_defaults,
};
use rust_clasp::clasp::cli::clasp_options::parse_config_key;

fn collect_configs(
    mut iter: rust_clasp::clasp::cli::clasp_cli_configs::ConfigIter,
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
fn defaults_follow_upstream_problem_type_mapping() {
    assert_eq!(get_defaults(ProblemType::Asp), "--configuration=tweety");
    assert_eq!(get_defaults(ProblemType::Sat), "--configuration=trendy");
    assert_eq!(get_defaults(ProblemType::Pb), "--configuration=trendy");
}

#[test]
fn config_keys_cover_default_and_auxiliary_presets() {
    let cases = [
        ("auto", ConfigKey::Default, 0),
        ("tweety", ConfigKey::Tweety, 1),
        ("trendy", ConfigKey::Trendy, 2),
        ("frumpy", ConfigKey::Frumpy, 3),
        ("crafty", ConfigKey::Crafty, 4),
        ("jumpy", ConfigKey::Jumpy, 5),
        ("handy", ConfigKey::Handy, 6),
        ("s6", ConfigKey::S6, 8),
        ("s7", ConfigKey::S7, 9),
        ("s8", ConfigKey::S8, 10),
        ("s9", ConfigKey::S9, 11),
        ("s10", ConfigKey::S10, 12),
        ("s11", ConfigKey::S11, 13),
        ("s12", ConfigKey::S12, 14),
        ("s13", ConfigKey::S13, 15),
        ("nolearn", ConfigKey::Nolearn, 16),
        ("tester", ConfigKey::Tester, 17),
        ("many", ConfigKey::Many, 19),
    ];

    for (name, expected, value) in cases {
        assert_eq!(parse_config_key(name).unwrap(), expected);
        assert_eq!(expected.as_u8(), value);
        assert_eq!(ConfigKey::from_u8(value), Some(expected));
    }

    assert_eq!(ConfigKey::DEFAULT_MAX_VALUE, 7);
    assert_eq!(ConfigKey::AUX_MAX_VALUE, 18);
    assert_eq!(ConfigKey::MAX_VALUE, 20);
    assert_eq!(ConfigKey::ASP_DEFAULT, ConfigKey::Tweety);
    assert_eq!(ConfigKey::SAT_DEFAULT, ConfigKey::Trendy);
    assert_eq!(ConfigKey::TESTER_DEFAULT, ConfigKey::Tester);
}

#[test]
fn config_catalog_matches_upstream_entries() {
    assert_eq!(DEFAULT_CONFIGS.len(), 6);
    assert_eq!(AUX_CONFIGS.len(), 10);

    let tweety = config_entry(ConfigKey::Tweety).unwrap();
    assert_eq!(tweety.solver_id, 0);
    assert_eq!(tweety.name, "tweety");
    assert_eq!(tweety.standalone, "--eq=3 --trans-ext=dynamic");
    assert_eq!(tweety.portfolio, "--opt-strat=bb,hier");
    assert!(tweety.common.contains("--save-progress=160"));

    let tester = config_entry(ConfigKey::Tester).unwrap();
    assert_eq!(tester.solver_id, 15);
    assert_eq!(tester.name, "tester");
    assert_eq!(tester.standalone, "--sat-p=2,iter=10,occ=25,time=240");
    assert_eq!(tester.portfolio, "");
    assert!(tester.common.contains("--counter-restarts=7,1023"));
}

#[test]
fn named_and_portfolio_config_iteration_matches_upstream_layout() {
    let trendy = collect_configs(get_config(ConfigKey::Trendy));
    assert_eq!(trendy.len(), 1);
    assert_eq!(trendy[0].0, "[trendy]");
    assert_eq!(trendy[0].1, "");
    assert!(
        trendy[0]
            .2
            .starts_with("--sat-p=2,iter=20,occ=25,time=240 --trans-ext=dynamic ")
    );
    assert!(trendy[0].2.contains("--counter-restarts=3,1023"));

    let many = collect_configs(get_config(ConfigKey::Many));
    assert_eq!(many.len(), 16);
    assert_eq!(many[0].0, "[solver.0]");
    assert_eq!(many[0].1, "");
    assert!(many[0].2.contains("--opt-strat=bb,hier"));
    assert_eq!(many[15].0, "[solver.15]");
    assert!(
        many[15]
            .2
            .starts_with("--heuristic=Vsids --restarts=D,100,0.7")
    );

    let default_only = collect_configs(get_config(ConfigKey::Default));
    assert_eq!(
        default_only,
        vec![("default".to_owned(), String::new(), String::new())]
    );
}
