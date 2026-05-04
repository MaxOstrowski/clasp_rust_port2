use std::panic::{self, AssertUnwindSafe};

use rust_clasp::clasp::claspfwd::Asp;
use rust_clasp::clasp::shared_context::ProblemStats;
use rust_clasp::clasp::solver_types::SolverStats;
use rust_clasp::clasp::statistics::{
    ClaspStatistics, Operation, StatisticArray, StatisticArrayElements, StatisticObject,
    StatisticObjectTypeId, StatsVisitor,
};
use rust_clasp::potassco::clingo::StatisticsType;
use rust_clasp::potassco::error::Error;

fn catch_error<F>(func: F) -> Error
where
    F: FnOnce(),
{
    let payload = panic::catch_unwind(AssertUnwindSafe(func)).expect_err("expected panic");
    *payload
        .downcast::<Error>()
        .expect("expected potassco error")
}

#[test]
fn problem_stats_match_upstream_diff_and_lookup() {
    let mut lhs = ProblemStats::default();
    let mut rhs = ProblemStats::default();

    lhs.vars.num = 100;
    rhs.vars.num = 150;
    lhs.vars.eliminated = 20;
    rhs.vars.eliminated = 30;
    lhs.constraints.other = 150;
    rhs.constraints.other = 150;
    lhs.constraints.binary = 0;
    rhs.constraints.binary = 100;
    lhs.constraints.ternary = 100;
    rhs.constraints.ternary = 0;

    lhs.diff(&rhs);

    assert_eq!(lhs.vars.num, 50);
    assert_eq!(lhs.vars.eliminated, 10);
    assert_eq!(lhs.constraints.other, 0);
    assert_eq!(lhs.constraints.binary, 100);
    assert_eq!(lhs.constraints.ternary, 100);

    let stats = StatisticObject::map(&lhs);
    assert_eq!(stats.size(), ProblemStats::size());
    assert_eq!(stats.at("vars").value(), 50.0);
    assert_eq!(stats.at("constraints").value(), 0.0);
    assert_eq!(stats.at("constraints_binary").value(), 100.0);
    assert_eq!(stats.at("constraints_ternary").value(), 100.0);
}

#[derive(Default)]
struct CountingVisitor {
    logic_program: u32,
    problem: u32,
    solver: u32,
    external: u32,
}

impl StatsVisitor for CountingVisitor {
    fn visit_logic_program_stats(&mut self, _stats: &Asp::LpStats) {
        self.logic_program += 1;
    }

    fn visit_problem_stats(&mut self, _stats: &ProblemStats) {
        self.problem += 1;
    }

    fn visit_solver_stats(&mut self, _stats: &SolverStats) {
        self.solver += 1;
    }

    fn visit_external_stats(&mut self, _stats: StatisticObject<'_>) {
        self.external += 1;
    }
}

#[test]
fn stats_visitor_default_helpers_match_upstream_dispatch() {
    let problem = ProblemStats::default();
    let solver = SolverStats::default();
    let mut visitor = CountingVisitor::default();

    assert!(visitor.visit_generator(Operation::Enter));
    assert!(visitor.visit_threads(Operation::Leave));
    assert!(visitor.visit_tester(Operation::Enter));
    assert!(visitor.visit_hccs(Operation::Leave));

    visitor.visit_thread(0, &solver);
    visitor.visit_hcc(1, &problem, &solver);

    assert_eq!(visitor.logic_program, 0);
    assert_eq!(visitor.problem, 1);
    assert_eq!(visitor.solver, 2);
    assert_eq!(visitor.external, 0);
}

#[test]
fn clasp_statistics_match_upstream_external_solver_stats_lookup() {
    let mut solver = SolverStats::default();
    assert!(solver.enable_extended());
    solver.core.choices = 100;
    solver.extra.as_mut().expect("extended stats").learnts[1] = 5;
    solver.extra.as_mut().expect("extended stats").binary = 6;

    let mut stats = ClaspStatistics::new();
    let root = stats.add_object(stats.root(), "test", StatisticObject::map(&solver), false);

    assert_eq!(stats.type_of(root), StatisticsType::Map);
    assert!(!stats.writable(root));

    let choices = stats.get(root, "choices");
    assert_eq!(stats.type_of(choices), StatisticsType::Value);
    assert_eq!(stats.value(choices), 100.0);

    let extra = stats.get(root, "extra");
    assert_eq!(stats.type_of(extra), StatisticsType::Map);

    let binary = stats.get(extra, "lemmas_binary");
    assert_eq!(stats.type_of(binary), StatisticsType::Value);
    assert_eq!(stats.value(binary), 6.0);

    let binary_by_path = stats.get(root, "extra.lemmas_binary");
    assert_eq!(binary_by_path, binary);
}

#[test]
fn clasp_statistics_visit_external_forwards_root_object() {
    struct ExternalVisitor {
        seen: u32,
        choices: f64,
    }

    impl StatsVisitor for ExternalVisitor {
        fn visit_logic_program_stats(&mut self, _stats: &Asp::LpStats) {}

        fn visit_problem_stats(&mut self, _stats: &ProblemStats) {}

        fn visit_solver_stats(&mut self, _stats: &SolverStats) {}

        fn visit_external_stats(&mut self, stats: StatisticObject<'_>) {
            self.seen += 1;
            self.choices = stats.at("choices").value();
        }
    }

    let mut solver = SolverStats::default();
    solver.core.choices = 13;

    let mut stats = ClaspStatistics::new();
    let _ = stats.add_object(stats.root(), "solver", StatisticObject::map(&solver), false);
    let mut visitor = ExternalVisitor {
        seen: 0,
        choices: 0.0,
    };

    assert!(stats.visit_external("solver", &mut visitor));
    assert!(!stats.visit_external("missing", &mut visitor));
    assert_eq!(visitor.seen, 1);
    assert_eq!(visitor.choices, 13.0);
}

#[test]
fn clasp_statistics_writable_state_matches_upstream_behaviour() {
    let mut stats = ClaspStatistics::new();
    let root = stats.root();
    let fixed_value = 2.0;
    let fixed = stats.add_object(
        root,
        "fixed",
        StatisticObject::from_value(&fixed_value),
        false,
    );

    assert_eq!(stats.get(root, "fixed"), fixed);
    assert!(!stats.writable(stats.get(root, "fixed")));
    assert_eq!(
        stats.add_object(
            root,
            "fixed",
            StatisticObject::from_value(&fixed_value),
            false
        ),
        fixed
    );

    let mismatch = catch_error(|| {
        let _ = stats.add_object(root, "fixed", StatisticObject::default(), false);
    });
    assert_eq!(
        mismatch,
        Error::InvalidArgument("unexpected object for key 'fixed'".to_owned())
    );

    let unreachable = stats.add_object(root, "fixed", StatisticObject::default(), true);
    assert_ne!(unreachable, fixed);
    assert_eq!(stats.get(root, "fixed"), fixed);
    assert_eq!(stats.value(unreachable), 0.0);

    let mutable = stats.add(root, "mutable", StatisticsType::Value);
    let mutable2 = stats.add(root, "mutable2", StatisticsType::Value);
    let mutable3 = stats.add(root, "mutable3", StatisticsType::Value);
    assert!(stats.writable(mutable));
    stats.set(mutable, 22.0);
    assert_eq!(stats.value(mutable), 22.0);

    let mut found = root;
    assert!(stats.find(root, "mutable", Some(&mut found)));
    assert_eq!(found, mutable);
    assert!(stats.find(root, "mutable3", Some(&mut found)));
    assert_eq!(found, mutable3);
    assert!(stats.find(root, "mutable2", Some(&mut found)));
    assert_eq!(found, mutable2);

    let array = stats.add(root, "array", StatisticsType::Array);
    assert_eq!(stats.type_of(array), StatisticsType::Array);
    assert!(stats.writable(array));
    assert_eq!(stats.size(array), 0);

    let map_at_arr0 = stats.push(array, StatisticsType::Map);
    assert_eq!(stats.type_of(map_at_arr0), StatisticsType::Map);
    assert_eq!(stats.size(array), 1);

    stats.freeze(true);
    let frozen = catch_error(|| {
        let _ = stats.value(fixed);
    });
    assert_eq!(
        frozen,
        Error::InvalidArgument("statistics not (yet) accessible".to_owned())
    );
    assert_eq!(stats.value(mutable), 22.0);
    stats.freeze(false);
    assert_eq!(stats.value(fixed), 2.0);
    assert_eq!(stats.value(mutable), 22.0);

    let _ = stats.add(map_at_arr0, "val", StatisticsType::Value);
    let _ = stats.add(map_at_arr0, "map", StatisticsType::Map);
    let _ = stats.add(map_at_arr0, "array", StatisticsType::Array);

    let bad_val_path = catch_error(|| {
        let _ = stats.get(map_at_arr0, "val.blub");
    });
    assert_eq!(
        bad_val_path,
        Error::OutOfRange("bad stats access: invalid key 'blub' in path 'val.blub'".to_owned())
    );

    let bad_map_path = catch_error(|| {
        let _ = stats.get(map_at_arr0, "map.blub");
    });
    assert_eq!(
        bad_map_path,
        Error::OutOfRange("bad stats access: invalid key 'blub' in path 'map.blub'".to_owned())
    );

    let bad_array_path = catch_error(|| {
        let _ = stats.get(map_at_arr0, "array.0");
    });
    assert_eq!(
        bad_array_path,
        Error::OutOfRange("bad stats access: invalid key '0' in path 'array.0'".to_owned())
    );

    let bad_at = catch_error(|| {
        let _ = stats.at(array, 1);
    });
    assert_eq!(
        bad_at,
        Error::OutOfRange(
            "bad stats access: index '1' is out of range for object of size '1'".to_owned()
        )
    );

    let bad_key = catch_error(|| {
        let _ = stats.key(map_at_arr0, 10);
    });
    assert_eq!(
        bad_key,
        Error::OutOfRange(
            "bad stats access: index '10' is out of range for object of size '3'".to_owned()
        )
    );

    let bad_value = catch_error(|| {
        let _ = stats.value(map_at_arr0);
    });
    assert_eq!(
        bad_value,
        Error::InvalidArgument("bad stats access: 'value' expected but got 'map'".to_owned())
    );
}

#[test]
fn clasp_statistics_keys_stay_stable_across_growth() {
    let mut stats = ClaspStatistics::new();
    let map = stats.add(stats.root(), "foo", StatisticsType::Map);
    let value = stats.add(map, "val1", StatisticsType::Value);
    stats.set(value, 123.2);

    let before = stats.key(map, 0);
    let before_ptr = before.as_ptr();
    assert_eq!(before, "val1");

    for index in 0..10 {
        let name = format!("more-{index}");
        let _ = stats.add(map, &name, StatisticsType::Value);
    }

    assert_eq!(stats.size(map), 11);
    assert_eq!(stats.value(value), 123.2);

    let mut after = "";
    for index in 0..11 {
        let key = stats.key(map, index);
        if key == "val1" {
            after = key;
            break;
        }
    }

    assert_eq!(after, "val1");
    assert_eq!(after.as_ptr(), before_ptr);
}

struct PairStats([u64; 2]);

impl StatisticArray for PairStats {
    fn size(&self) -> u32 {
        self.0.len() as u32
    }

    fn at<'a>(&'a self, index: u32) -> StatisticObject<'a> {
        StatisticObject::from_value(&self.0[index as usize])
    }
}

struct NamedValue {
    value: u64,
}

struct NamedValueArray([NamedValue; 2]);

impl StatisticArrayElements for NamedValueArray {
    type Item = NamedValue;

    fn size(&self) -> u32 {
        self.0.len() as u32
    }

    fn item(&self, index: u32) -> &Self::Item {
        &self.0[index as usize]
    }
}

fn named_value_stat<'a>(item: &'a NamedValue) -> StatisticObject<'a> {
    StatisticObject::from_value(&item.value)
}

#[test]
fn compatibility_wrappers_preserve_statistics_surface_behavior() {
    let value_a = 7u64;
    let value_b = 11u64;
    let value_c = 3u32;
    let lhs = StatisticObject::from_value(&value_a);
    let rhs_same_type = StatisticObject::from_value(&value_b);
    let rhs_other_type = StatisticObject::from_value(&value_c);
    let inline = StatisticObject::from_f64(1.5);

    assert_eq!(lhs.type_id(), rhs_same_type.type_id());
    assert!(lhs.eq_type_id(&rhs_same_type));
    assert_eq!(inline.type_id(), StatisticObjectTypeId::InlineValue);
    assert_eq!(lhs.r#type(), StatisticsType::Value);
    assert!(!lhs.eq_type_id(&rhs_other_type));
    assert_eq!(lhs.object(), core::ptr::from_ref(&value_a).cast::<()>());
    assert_eq!(
        rhs_same_type.object(),
        core::ptr::from_ref(&value_b).cast::<()>()
    );
    assert!(inline.object().is_null());

    let array = PairStats([2, 9]);
    let array_obj = StatisticObject::array(&array);
    assert_eq!(array_obj.r#type(), StatisticsType::Array);
    assert_eq!(array_obj.at_index(1).value(), 9.0);
    assert_eq!(array_obj.object(), core::ptr::from_ref(&array).cast::<()>());

    let mapped = NamedValueArray([NamedValue { value: 4 }, NamedValue { value: 8 }]);
    let mapped_array = StatisticObject::array_with(&mapped, named_value_stat);
    assert_eq!(mapped_array.r#type(), StatisticsType::Array);
    assert_eq!(mapped_array.at_index(0).value(), 4.0);
    assert_eq!(mapped_array.at_index(1).value(), 8.0);
    assert_eq!(
        mapped_array.object(),
        core::ptr::from_ref(&mapped).cast::<()>()
    );

    struct ExternalVisitor {
        seen: u32,
        value: f64,
    }

    impl StatsVisitor for ExternalVisitor {
        fn visit_logic_program_stats(&mut self, _stats: &Asp::LpStats) {}

        fn visit_problem_stats(&mut self, _stats: &ProblemStats) {}

        fn visit_solver_stats(&mut self, _stats: &SolverStats) {}

        fn visit_external_stats(&mut self, stats: StatisticObject<'_>) {
            self.seen += 1;
            self.value = stats.value();
        }
    }

    let mut stats = ClaspStatistics::new();
    let root = stats.root();
    let key = stats.addObject(root, "fixed", lhs, false);
    assert_eq!(stats.value(key), 7.0);

    let mut visitor = ExternalVisitor {
        seen: 0,
        value: 0.0,
    };
    assert!(stats.visitExternal("fixed", &mut visitor));
    assert!(!stats.visitExternal("missing", &mut visitor));
    assert_eq!(visitor.seen, 1);
    assert_eq!(visitor.value, 7.0);
}

#[test]
fn statistic_object_assignment_matches_upstream_reset_behavior() {
    let def = StatisticObject::default();
    let value = 32u64;
    let mut stat = StatisticObject::from_value(&value);

    assert_eq!(def.r#type(), StatisticsType::Value);
    assert_eq!(def.value(), 0.0);
    assert_eq!(stat.object(), core::ptr::from_ref(&value).cast::<()>());
    assert!(!stat.eq_type_id(&def));
    assert_eq!(stat.r#type(), StatisticsType::Value);
    assert_eq!(stat.value(), 32.0);

    stat = StatisticObject::default();

    assert_eq!(stat.r#type(), StatisticsType::Value);
    assert_eq!(stat.value(), 0.0);
    assert!(stat.eq_type_id(&def));
}

#[test]
fn statistic_object_order_matches_upstream_identity_comparison() {
    let value_a = 5u64;
    let value_b = 9u64;
    let lhs = StatisticObject::from_value(&value_a);
    let rhs = StatisticObject::from_value(&value_b);
    let expected =
        (lhs.object() as usize, lhs.type_id()).cmp(&(rhs.object() as usize, rhs.type_id()));

    assert_eq!(lhs.cmp(&rhs), expected);

    let inline_lhs = StatisticObject::from_f64(1.0);
    let inline_rhs = StatisticObject::from_f64(2.0);
    assert!(inline_lhs < inline_rhs);
}

#[test]
fn clasp_statistics_type_wrapper_matches_upstream_surface() {
    let fixed = 7u64;
    let mut stats = ClaspStatistics::new();
    let root = stats.root();
    let mutable = stats.add(root, "mutable", StatisticsType::Value);
    let external = stats.addObject(root, "fixed", StatisticObject::from_value(&fixed), false);

    assert_eq!(stats.r#type(root), StatisticsType::Map);
    assert_eq!(stats.r#type(mutable), StatisticsType::Value);
    assert_eq!(stats.r#type(external), StatisticsType::Value);
}
