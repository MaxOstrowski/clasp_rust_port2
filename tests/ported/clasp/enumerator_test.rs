use rust_clasp::clasp::cli::clasp_cli_options::ProjectMode;
use rust_clasp::clasp::enumerator::{
    EnumMode, EnumOptions, Model, ModelType, OutputPredicate, OutputProjection, model_type,
};
use rust_clasp::clasp::literal::{SumVec, pos_lit, value_false, value_true};
use rust_clasp::clasp::minimize_constraint::MinimizeMode;

#[test]
fn enum_options_default_matches_upstream_constructor() {
    let options = EnumOptions::default();

    assert_eq!(options.num_models, -1);
    assert_eq!(options.enum_mode, EnumMode::Auto);
    assert_eq!(options.opt_mode, MinimizeMode::Optimize);
    assert_eq!(options.pro_mode, ProjectMode::Implicit);
    assert_eq!(options.project, 0);
    assert!(options.opt_bound.is_empty());
    assert!(options.opt_stop.is_empty());
}

#[test]
fn enum_options_consequences_matches_upstream_mode_bit_test() {
    assert!(!EnumOptions::default().consequences());
    assert!(
        !EnumOptions {
            enum_mode: EnumMode::Record,
            ..EnumOptions::default()
        }
        .consequences()
    );
    assert!(
        EnumOptions {
            enum_mode: EnumMode::Brave,
            ..EnumOptions::default()
        }
        .consequences()
    );
    assert!(
        EnumOptions {
            enum_mode: EnumMode::Cautious,
            ..EnumOptions::default()
        }
        .consequences()
    );
    assert!(
        EnumOptions {
            enum_mode: EnumMode::Query,
            ..EnumOptions::default()
        }
        .consequences()
    );
    assert!(
        !EnumOptions {
            enum_mode: EnumMode::User,
            ..EnumOptions::default()
        }
        .consequences()
    );
}

#[test]
fn enum_options_models_matches_upstream_mode_ordering() {
    assert!(EnumOptions::default().models());
    assert!(
        EnumOptions {
            enum_mode: EnumMode::Bt,
            ..EnumOptions::default()
        }
        .models()
    );
    assert!(
        EnumOptions {
            enum_mode: EnumMode::DomRecord,
            ..EnumOptions::default()
        }
        .models()
    );
    assert!(
        !EnumOptions {
            enum_mode: EnumMode::Consequences,
            ..EnumOptions::default()
        }
        .models()
    );
    assert!(
        !EnumOptions {
            enum_mode: EnumMode::Query,
            ..EnumOptions::default()
        }
        .models()
    );
    assert!(
        !EnumOptions {
            enum_mode: EnumMode::User,
            ..EnumOptions::default()
        }
        .models()
    );
}

#[test]
fn enum_options_optimize_matches_upstream_minimize_mode_bit_test() {
    assert!(EnumOptions::default().optimize());
    assert!(
        EnumOptions {
            opt_mode: MinimizeMode::EnumOpt,
            ..EnumOptions::default()
        }
        .optimize()
    );
    assert!(
        !EnumOptions {
            opt_mode: MinimizeMode::Enumerate,
            ..EnumOptions::default()
        }
        .optimize()
    );
    assert!(
        !EnumOptions {
            opt_mode: MinimizeMode::Ignore,
            ..EnumOptions::default()
        }
        .optimize()
    );
}

#[test]
fn model_helpers_report_assignment_and_consequence_flags() {
    let values = [
        value_true,
        value_true | Model::est_mask(pos_lit(1)),
        value_false,
    ];
    assert!(!Model::new().consequences());

    let mut model = Model::with_values(&values);
    model.model_type = ModelType::Cautious as u32;

    assert!(model.consequences());
    assert!(model.has_var(2));
    assert_eq!(model.value(0), value_true);
    assert!(model.is_true(pos_lit(0)));
    assert!(model.is_true(pos_lit(1)));
    assert!(model.is_est(pos_lit(1)));
    assert_eq!(
        model.is_cons(pos_lit(1)),
        rust_clasp::clasp::literal::value_free
    );
    assert!(!model.is_def(pos_lit(1)));

    model.def = true;
    assert!(!model.is_est(pos_lit(1)));
    assert!(model.is_def(pos_lit(1)));
}

#[test]
fn model_has_costs_reflects_cost_view_presence() {
    let mut model = Model::new();
    assert!(!model.has_costs());

    let mut costs = SumVec::new();
    costs.push_back(3);
    costs.push_back(1);
    model.set_costs(&costs);

    assert!(model.has_costs());
}

#[test]
fn num_consequences_counts_output_mode_literals_like_upstream() {
    let values = [
        value_true,
        value_true | Model::est_mask(pos_lit(1)),
        value_false,
        value_true,
    ];
    let model = Model {
        values: &values,
        model_type: ModelType::Cautious as u32,
        ..Model::new()
    };
    let predicates = [
        OutputPredicate {
            cond: pos_lit(0),
            name: "a",
        },
        OutputPredicate {
            cond: pos_lit(1),
            name: "b",
        },
    ];
    let vars = [2, 3];
    let output = OutputProjection::new(ProjectMode::Output, &predicates, &vars, &[]);

    assert_eq!(model.num_consequences(&output), (2, 1));
}

#[test]
fn num_consequences_uses_explicit_projection_when_not_in_output_mode() {
    let values = [
        value_true,
        value_true | Model::est_mask(pos_lit(1)),
        value_false,
    ];
    let mut model = Model {
        values: &values,
        model_type: ModelType::Cautious as u32,
        ..Model::new()
    };
    let projected = [pos_lit(0), pos_lit(1), pos_lit(2)];
    let output = OutputProjection::new(ProjectMode::Project, &[], &[], &projected);

    assert_eq!(model.num_consequences(&output), (1, 1));

    model.def = true;
    assert_eq!(model.num_consequences(&output), (2, 0));
}

#[test]
fn model_type_text_matches_upstream_labels() {
    let mut model = Model::new();
    assert_eq!(model_type(&model), Some("Model"));

    model.model_type = ModelType::Brave as u32;
    assert_eq!(model_type(&model), Some("Brave"));

    model.model_type = ModelType::Cautious as u32;
    assert_eq!(model_type(&model), Some("Cautious"));

    model.model_type = ModelType::User as u32;
    assert_eq!(model_type(&model), Some("User"));

    model.model_type = 99;
    assert_eq!(model_type(&model), None);
}
