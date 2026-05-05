use rust_clasp::clasp::cli::clasp_output::{CatAssign, CatAtom, CatCost, CatStep, CatTemplate};
use rust_clasp::potassco::basic_types::AtomArg;

#[test]
fn cat_atom_default_layout_matches_upstream_members() {
    let cat = CatAtom::new();

    assert_eq!(cat.buffer(), "");
    assert_eq!(cat.atom_sep(), u32::MAX);
    assert_eq!(cat.var_start(), u32::MAX);
    assert_eq!(cat.var_sep(), u32::MAX);
    assert!(!cat.has_atom());
    assert!(!cat.has_var());
    assert!(!cat.active());
}

#[test]
fn cat_atom_reports_atom_and_var_presence_from_member_slots() {
    let mut cat = CatAtom::new();
    cat.set_layout_for_test("%0:%1", 2, 3, 4);

    assert_eq!(cat.buffer(), "%0:%1");
    assert!(cat.has_atom());
    assert!(cat.has_var());
    assert!(cat.active());
}

#[test]
fn cat_template_default_layout_matches_upstream_members() {
    let template = CatTemplate::new();

    assert_eq!(template.data(), "");
    assert_eq!(template.cap_start(), 0);
    assert_eq!(template.fmt_start(), 0);
    assert_eq!(template.arity(), 0);
    assert_eq!(template.max_arg(), 0);
    assert!(!template.active());
}

#[test]
fn cat_template_tracks_full_member_state() {
    let mut template = CatTemplate::new();
    template.set_layout_for_test("pred/cap/fmt", 4, 8, 2, 1);

    assert_eq!(template.data(), "pred/cap/fmt");
    assert_eq!(template.cap_start(), 4);
    assert_eq!(template.fmt_start(), 8);
    assert_eq!(template.arity(), 2);
    assert_eq!(template.max_arg(), 1);
    assert!(template.active());
}

#[test]
fn cat_assign_and_cost_alias_the_template_layout() {
    let assign = CatAssign::new();
    let cost = CatCost::new();

    assert_eq!(assign, CatTemplate::new());
    assert_eq!(cost, CatTemplate::new());
}

#[test]
fn cat_step_default_and_member_state_match_upstream_layout() {
    let mut step = CatStep::new();

    assert_eq!(step.arg_name(), "");
    assert_eq!(step.step_arg(), AtomArg::Last);
    assert!(!step.active());

    step.set_layout_for_test("State", AtomArg::First, true);
    assert_eq!(step.arg_name(), "State");
    assert_eq!(step.step_arg(), AtomArg::First);
    assert!(step.active());
}
