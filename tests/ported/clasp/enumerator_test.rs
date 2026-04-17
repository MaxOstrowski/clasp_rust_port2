use rust_clasp::clasp::cli::clasp_cli_options::ProjectMode;
use rust_clasp::clasp::enumerator::{
    Model, ModelType, OutputPredicate, OutputProjection, model_type,
};
use rust_clasp::clasp::literal::{pos_lit, value_false, value_true};

#[test]
fn model_helpers_report_assignment_and_consequence_flags() {
    let values = [
        value_true,
        value_true | Model::est_mask(pos_lit(1)),
        value_false,
    ];
    let mut model = Model::with_values(&values);
    model.model_type = ModelType::Cautious as u32;

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
    assert_eq!(model_type(&model), "Model");

    model.model_type = ModelType::Brave as u32;
    assert_eq!(model_type(&model), "Brave consequences");

    model.model_type = ModelType::Cautious as u32;
    assert_eq!(model_type(&model), "Cautious consequences");
}
