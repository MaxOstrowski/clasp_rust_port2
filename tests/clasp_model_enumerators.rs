use rust_clasp::clasp::cb_enumerator::{EnumMode, EnumOptions};
use rust_clasp::clasp::model_enumerators::{
    ModelEnumerator, ModelEnumeratorInitWarning, ProjectOptions, Strategy,
};

#[test]
fn set_strategy_normalizes_projection_flags_like_upstream() {
    let mut enumerator = ModelEnumerator::new(Strategy::Auto);

    enumerator.set_strategy(
        Strategy::Backtrack,
        ProjectOptions::UseHeuristic as u32 | ProjectOptions::SaveProgress as u32,
        '#',
    );

    assert_eq!(enumerator.strategy(), Strategy::Backtrack);
    assert_eq!(enumerator.filter(), '#');
    assert!(enumerator.projection_enabled());
    assert_eq!(
        enumerator.project_options(),
        ProjectOptions::EnableSimple as u32
            | ProjectOptions::UseHeuristic as u32
            | ProjectOptions::SaveProgress as u32
    );
}

#[test]
fn set_strategy_truncates_unknown_projection_bits_like_upstream_bitfields() {
    let mut enumerator = ModelEnumerator::new(Strategy::Auto);

    enumerator.set_strategy(Strategy::Record, u32::MAX, '_');

    // Only the 5 known projection option bits may remain set.
    assert_eq!(enumerator.project_options() & !0x1f, 0);
    assert_eq!(enumerator.project_options(), 0x1f);
}

#[test]
fn from_enum_options_maps_bt_record_and_dom_record_modes() {
    let backtrack = ModelEnumerator::from_enum_options(
        EnumOptions {
            enum_mode: EnumMode::Bt,
        },
        0,
    );
    let record = ModelEnumerator::from_enum_options(
        EnumOptions {
            enum_mode: EnumMode::Record,
        },
        0,
    );
    let dom_record = ModelEnumerator::from_enum_options(
        EnumOptions {
            enum_mode: EnumMode::DomRecord,
        },
        0,
    );

    assert_eq!(backtrack.strategy(), Strategy::Backtrack);
    assert_eq!(record.strategy(), Strategy::Record);
    assert_eq!(dom_record.strategy(), Strategy::Record);
    assert!(dom_record.dom_rec());
}

#[test]
fn project_membership_uses_dynamic_bitset_storage() {
    let mut enumerator = ModelEnumerator::default();

    assert!(!enumerator.project(1));
    assert!(enumerator.add_project(1));
    assert!(enumerator.add_project(65));
    assert!(!enumerator.add_project(65));
    assert!(enumerator.project(1));
    assert!(enumerator.project(65));
    assert!(!enumerator.project(64));

    enumerator.clear_project();

    assert!(!enumerator.project(1));
    assert!(!enumerator.project(65));
}

#[test]
fn support_predicates_follow_upstream_conditions() {
    let record = ModelEnumerator::new(Strategy::Record);
    let mut projected_backtrack = ModelEnumerator::new(Strategy::Backtrack);
    let mut dom_record = ModelEnumerator::new(Strategy::Record);

    projected_backtrack.set_strategy(
        Strategy::Backtrack,
        ProjectOptions::EnableSimple as u32,
        '_',
    );
    dom_record.set_strategy(Strategy::Record, ProjectOptions::DomLits as u32, '_');

    assert!(record.supports_restarts(false));
    assert!(!projected_backtrack.supports_restarts(false));
    assert!(projected_backtrack.supports_restarts(true));

    assert!(record.supports_parallel());
    assert!(!projected_backtrack.supports_parallel());

    assert!(record.supports_splitting(true));
    assert!(!record.supports_splitting(false));
    assert!(!dom_record.supports_splitting(true));
    assert!(projected_backtrack.supports_splitting(true));
}

#[test]
fn auto_init_prefers_backtrack_record_and_warning_in_upstream_cases() {
    let mut baseline = ModelEnumerator::new(Strategy::Auto);
    let mut parallel_projection = ModelEnumerator::new(Strategy::Auto);
    let mut optimized = ModelEnumerator::new(Strategy::Auto);
    let mut projected_optimization = ModelEnumerator::new(Strategy::Auto);
    let mut dom_record = ModelEnumerator::new(Strategy::Auto);

    parallel_projection.set_strategy(Strategy::Auto, ProjectOptions::EnableSimple as u32, '_');
    optimized.set_strategy(Strategy::Auto, 0, '_');
    projected_optimization.set_strategy(Strategy::Auto, ProjectOptions::EnableSimple as u32, '_');
    dom_record.set_strategy(Strategy::Auto, ProjectOptions::DomLits as u32, '_');

    let baseline_init = baseline.prepare_for_init(false, 0, 1, false);
    let parallel_init = parallel_projection.prepare_for_init(false, 0, 2, false);
    let optimized_init = optimized.prepare_for_init(true, 0, 1, true);
    let warning_init = projected_optimization.prepare_for_init(true, 0, 1, false);
    let dom_init = dom_record.prepare_for_init(true, 0, 1, true);

    assert_eq!(baseline_init.strategy, Strategy::Backtrack);
    assert!(!baseline_init.trivial);

    assert_eq!(parallel_init.strategy, Strategy::Record);
    assert!(!parallel_init.trivial);

    assert_eq!(optimized_init.strategy, Strategy::Record);
    assert!(optimized_init.trivial);
    assert_eq!(optimized.trivial(), optimized_init.trivial);

    assert_eq!(warning_init.strategy, Strategy::Backtrack);
    assert!(!warning_init.trivial);
    assert_eq!(
        warning_init.warning,
        Some(ModelEnumeratorInitWarning::ProjectionMayDependOnEnumerationOrder)
    );

    assert_eq!(dom_init.strategy, Strategy::Backtrack);
    assert!(!dom_init.trivial);
    assert_eq!(dom_init.warning, None);
}
