use std::panic::{self, AssertUnwindSafe};

use rust_clasp::potassco::basic_types::{
    AbstractProgram, Atom, AtomSpan, BodyType, HeadType, Lit, LitSpan, Weight, WeightLit,
    WeightLitSpan,
};
use rust_clasp::potassco::error::Error;
use rust_clasp::potassco::rule_utils::{Rule, RuleBuilder};

fn catch_error<F>(func: F) -> Error
where
    F: FnOnce(),
{
    let payload = panic::catch_unwind(AssertUnwindSafe(func)).expect_err("expected panic");
    *payload
        .downcast::<Error>()
        .expect("expected potassco error")
}

#[derive(Debug, Eq, PartialEq)]
enum Event {
    Rule {
        head_type: HeadType,
        head: Vec<Atom>,
        body: Vec<Lit>,
    },
    WeightedRule {
        head_type: HeadType,
        head: Vec<Atom>,
        bound: Weight,
        body: Vec<WeightLit>,
    },
    Minimize {
        priority: Weight,
        body: Vec<WeightLit>,
    },
}

#[derive(Default)]
struct RecordingProgram {
    events: Vec<Event>,
}

impl AbstractProgram for RecordingProgram {
    fn rule(&mut self, head_type: HeadType, head: AtomSpan<'_>, body: LitSpan<'_>) {
        self.events.push(Event::Rule {
            head_type,
            head: head.to_vec(),
            body: body.to_vec(),
        });
    }

    fn rule_weighted(
        &mut self,
        head_type: HeadType,
        head: AtomSpan<'_>,
        bound: Weight,
        body: WeightLitSpan<'_>,
    ) {
        self.events.push(Event::WeightedRule {
            head_type,
            head: head.to_vec(),
            bound,
            body: body.to_vec(),
        });
    }

    fn minimize(&mut self, priority: Weight, lits: WeightLitSpan<'_>) {
        self.events.push(Event::Minimize {
            priority,
            body: lits.to_vec(),
        });
    }

    fn output_atom(&mut self, _atom: Atom, _name: &str) {
        panic!("not used in rule_utils tests");
    }
}

#[test]
fn rule_named_constructors_match_upstream_layout() {
    let head = [1, 2];
    let cond = [3, -4];
    let weighted = [WeightLit { lit: 5, weight: 2 }];

    let normal = Rule::normal(HeadType::Choice, &head, &cond);
    assert_eq!(normal.ht, HeadType::Choice);
    assert_eq!(normal.head, &head);
    assert_eq!(normal.bt, BodyType::Normal);
    assert_eq!(normal.cond, &cond);
    assert!(normal.is_normal());
    assert!(!normal.is_sum());

    let sum = Rule::sum_with_bound(HeadType::Disjunctive, &head[..1], 7, &weighted);
    assert_eq!(sum.bt, BodyType::Sum);
    assert_eq!(sum.agg.bound, 7);
    assert_eq!(sum.agg.lits, &weighted);
    assert!(sum.is_sum());
}

#[test]
fn builder_supports_fact_normal_constraint_and_choice_rules() {
    let mut rb = RuleBuilder::default();

    rb.start().add_head(1).end(None);
    assert!(rb.is_fact());
    assert_eq!(rb.head(), &[1]);
    assert_eq!(rb.body_type(), BodyType::Normal);
    assert_eq!(rb.body(), &[]);

    rb.start_body().add_goal(2).add_goal(-3).start().end(None);
    assert_eq!(rb.head(), &[]);
    assert_eq!(rb.body_type(), BodyType::Normal);
    assert_eq!(rb.body(), &[2, -3]);

    rb.start_body()
        .start_with_type(HeadType::Choice)
        .add_head(1)
        .add_head(2)
        .end(None);
    assert_eq!(rb.head(), &[1, 2]);
    assert_eq!(rb.body_type(), BodyType::Normal);
    assert_eq!(rb.body(), &[]);
}

#[test]
fn builder_supports_sum_rules_bound_updates_and_lookup() {
    let mut rb = RuleBuilder::default();

    rb.start()
        .add_head(1)
        .start_sum(2)
        .add_goal_with_weight(2, 1)
        .add_goal_with_weight(-3, 1)
        .add_goal_with_weight(4, 2)
        .end(None);

    assert_eq!(rb.head(), &[1]);
    assert_eq!(rb.body_type(), BodyType::Sum);
    assert_eq!(rb.bound(), 2);
    assert_eq!(
        rb.sum_lits(),
        &[
            WeightLit { lit: 2, weight: 1 },
            WeightLit { lit: -3, weight: 1 },
            WeightLit { lit: 4, weight: 2 }
        ]
    );
    assert_eq!(rb.find_sum_lit(4).map(|lit| lit.weight), Some(2));
    assert_eq!(rb.find_sum_lit(-4), None);

    let rule = rb.rule();
    assert_eq!(rule.head, rb.head());
    assert_eq!(rule.bt, BodyType::Sum);
    assert_eq!(rule.agg.bound, 2);
    assert_eq!(rule.agg.lits, rb.sum_lits());

    rb.clear()
        .start()
        .add_head(1)
        .start_sum(2)
        .add_goal_with_weight(2, 1)
        .add_goal_with_weight(-3, 0)
        .add_goal_with_weight(4, 2)
        .end(None);
    assert_eq!(
        rb.sum_lits(),
        &[
            WeightLit { lit: 2, weight: 1 },
            WeightLit { lit: 4, weight: 2 }
        ]
    );

    rb.clear()
        .start_sum(2)
        .add_goal_with_weight(2, 1)
        .add_goal_with_weight(-3, 1)
        .add_goal_with_weight(4, 2)
        .add_head(1)
        .set_bound(4);
    assert_eq!(rb.bound(), 4);

    rb.end(None);
    let error = catch_error(|| {
        rb.set_bound(5);
    });
    assert!(
        matches!(error, Error::InvalidArgument(message) if message.contains("Invalid call to setBound"))
    );
}

#[test]
fn builder_supports_weaken_minimize_and_clearing() {
    let mut rb = RuleBuilder::default();

    rb.start()
        .add_head(1)
        .start_sum(2)
        .add_goal_with_weight(2, 2)
        .add_goal_with_weight(-3, 2)
        .add_goal_with_weight(4, 2)
        .weaken(BodyType::Count, true)
        .end(None);
    assert_eq!(rb.body_type(), BodyType::Count);
    assert_eq!(rb.bound(), 1);
    assert_eq!(
        rb.sum_lits(),
        &[
            WeightLit { lit: 2, weight: 1 },
            WeightLit { lit: -3, weight: 1 },
            WeightLit { lit: 4, weight: 1 }
        ]
    );

    rb.start_sum(3)
        .add_goal_with_weight(2, 2)
        .add_goal_with_weight(-3, 2)
        .add_goal_with_weight(4, 2)
        .start()
        .add_head(1)
        .weaken(BodyType::Normal, true)
        .end(None);
    assert_eq!(rb.body_type(), BodyType::Normal);
    assert_eq!(rb.body(), &[2, -3, 4]);

    rb.start_minimize(1)
        .start_sum(0)
        .add_goal_with_weight(-3, 2)
        .add_goal_with_weight(4, 1)
        .add_goal(5)
        .end(None);
    assert!(rb.is_minimize());
    assert_eq!(rb.bound(), 1);
    assert_eq!(
        rb.sum_lits(),
        &[
            WeightLit { lit: -3, weight: 2 },
            WeightLit { lit: 4, weight: 1 },
            WeightLit { lit: 5, weight: 1 }
        ]
    );

    rb.start()
        .add_head(1)
        .start_sum(3)
        .add_goal_with_weight(2, 2)
        .add_goal_with_weight(-3, 2)
        .add_goal_with_weight(4, 2)
        .clear_body()
        .start_body()
        .add_goal(5)
        .end(None);
    assert_eq!(rb.head(), &[1]);
    assert_eq!(rb.body_type(), BodyType::Normal);
    assert_eq!(rb.body(), &[5]);

    rb.start_sum(3)
        .add_goal_with_weight(2, 2)
        .add_goal_with_weight(-3, 2)
        .add_goal_with_weight(4, 2)
        .start()
        .add_head(1)
        .clear_head()
        .start()
        .add_head(5)
        .end(None);
    assert_eq!(rb.head(), &[5]);
    assert_eq!(rb.body_type(), BodyType::Sum);
    assert_eq!(
        rb.sum_lits(),
        &[
            WeightLit { lit: 2, weight: 2 },
            WeightLit { lit: -3, weight: 2 },
            WeightLit { lit: 4, weight: 2 }
        ]
    );
}

#[test]
fn builder_clone_and_end_output_follow_upstream_behavior() {
    let mut rb = RuleBuilder::default();
    rb.start().add_head(1).start_sum(25);
    for i in 2..20 {
        rb.add_weight_lit(WeightLit {
            lit: if i % 2 == 0 { -i } else { i },
            weight: i,
        });
    }

    let mut clone = rb.clone();
    clone.add_weight_lit(WeightLit {
        lit: 4711,
        weight: 31,
    });
    assert_eq!(rb.sum_lits().len(), 18);
    assert_eq!(clone.sum_lits().len(), 19);

    let mut out = RecordingProgram::default();
    clone.end(Some(&mut out));
    assert_eq!(out.events.len(), 1);
    assert!(matches!(
        &out.events[0],
        Event::WeightedRule {
            head_type: HeadType::Disjunctive,
            head,
            bound: 25,
            body,
        } if head == &vec![1] && body.last() == Some(&WeightLit { lit: 4711, weight: 31 })
    ));

    rb.clear()
        .start_minimize(0)
        .add_goal(1)
        .add_goal_with_weight(2, 2)
        .add_goal(3)
        .end(Some(&mut out));
    assert!(matches!(
        &out.events[1],
        Event::Minimize { priority: 0, body } if body
            == &vec![
                WeightLit { lit: 1, weight: 1 },
                WeightLit { lit: 2, weight: 2 },
                WeightLit { lit: 3, weight: 1 }
            ]
    ));
}

#[test]
fn builder_enforces_freeze_and_phase_preconditions() {
    let start_twice = catch_error(|| {
        let mut rb = RuleBuilder::default();
        rb.add_head(1).start();
    });
    assert!(
        matches!(start_twice, Error::InvalidArgument(message) if message.contains("Head already started"))
    );

    let add_after_switch = catch_error(|| {
        let mut rb = RuleBuilder::default();
        rb.start().add_head(1).add_goal(2).add_head(3);
    });
    assert!(
        matches!(add_after_switch, Error::InvalidArgument(message) if message.contains("Head already frozen"))
    );

    let weighted_normal = catch_error(|| {
        let mut rb = RuleBuilder::default();
        rb.start_body().add_goal_with_weight(2, 2);
    });
    assert!(
        matches!(weighted_normal, Error::InvalidArgument(message) if message.contains("non-trivial weight literal not supported in normal body"))
    );

    let weaken_minimize = catch_error(|| {
        let mut rb = RuleBuilder::default();
        rb.start_minimize(1).weaken(BodyType::Count, true);
    });
    assert!(
        matches!(weaken_minimize, Error::InvalidArgument(message) if message.contains("Invalid call to weaken"))
    );

    let mut rb = RuleBuilder::default();
    rb.start()
        .add_head(1)
        .add_goal(2)
        .end(None)
        .start()
        .add_head(3);
    assert_eq!(rb.head(), &[3]);
    assert_eq!(rb.body(), &[]);
}
