#[path = "../examples/ported/example.rs"]
mod example_ported;

use rust_clasp::clasp::enumerator::Model;
use rust_clasp::clasp::literal::{neg_lit, pos_lit, value_false, value_true};
use rust_clasp::clasp::shared_context::OutputTable;
use rust_clasp::clasp::util::misc_types::Range32;

#[test]
fn print_model_matches_upstream_text_layout() {
    let keep_public_api = example_ported::print_model;
    let _ = keep_public_api;

    let values = [0, value_true, value_false, value_true];
    let mut model = Model::with_values(&values);
    model.num = 7;

    let mut out = OutputTable::default();
    out.add_predicate("a", pos_lit(1), 0);
    out.add_predicate("not_b", neg_lit(2), 0);
    out.set_var_range(Range32::new(1, 4));

    let mut actual = Vec::new();
    example_ported::print_model_to(&mut actual, &out, &model).unwrap();

    assert_eq!(
        String::from_utf8(actual).unwrap(),
        "Model 7: \na not_b 1 -2 3 \n"
    );
}
