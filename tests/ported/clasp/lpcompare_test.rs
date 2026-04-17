use std::collections::BTreeMap;

use rust_clasp::potassco::clingo::{AbstractStatistics, StatisticsKey, StatisticsType};

use super::lpcompare::{
    add_external_stats, add_external_stats_under, compare_program, find_program,
};

#[derive(Clone, Debug)]
enum StatsNode {
    Value(f64),
    Array(Vec<StatisticsKey>),
    Map(BTreeMap<String, StatisticsKey>),
}

impl StatsNode {
    fn item_type(&self) -> StatisticsType {
        match self {
            Self::Value(_) => StatisticsType::Value,
            Self::Array(_) => StatisticsType::Array,
            Self::Map(_) => StatisticsType::Map,
        }
    }
}

#[derive(Debug)]
struct TestStats {
    nodes: Vec<StatsNode>,
}

impl Default for TestStats {
    fn default() -> Self {
        Self {
            nodes: vec![StatsNode::Map(BTreeMap::new())],
        }
    }
}

impl TestStats {
    fn node(&self, key: StatisticsKey) -> &StatsNode {
        self.nodes
            .get(key as usize)
            .expect("invalid statistics key")
    }

    fn node_mut(&mut self, key: StatisticsKey) -> &mut StatsNode {
        self.nodes
            .get_mut(key as usize)
            .expect("invalid statistics key")
    }

    fn push_node(&mut self, node: StatsNode) -> StatisticsKey {
        let key = self.nodes.len() as StatisticsKey;
        self.nodes.push(node);
        key
    }
}

impl AbstractStatistics for TestStats {
    fn root(&self) -> StatisticsKey {
        0
    }

    fn type_of(&self, key: StatisticsKey) -> StatisticsType {
        self.node(key).item_type()
    }

    fn size(&self, key: StatisticsKey) -> usize {
        match self.node(key) {
            StatsNode::Value(_) => 0,
            StatsNode::Array(items) => items.len(),
            StatsNode::Map(items) => items.len(),
        }
    }

    fn writable(&self, _key: StatisticsKey) -> bool {
        true
    }

    fn at(&self, array: StatisticsKey, index: usize) -> StatisticsKey {
        match self.node(array) {
            StatsNode::Array(items) => items[index],
            _ => panic!("statistics key is not an array"),
        }
    }

    fn push(&mut self, array: StatisticsKey, item_type: StatisticsType) -> StatisticsKey {
        let key = self.push_node(match item_type {
            StatisticsType::Value => StatsNode::Value(0.0),
            StatisticsType::Array => StatsNode::Array(Vec::new()),
            StatisticsType::Map => StatsNode::Map(BTreeMap::new()),
        });
        match self.node_mut(array) {
            StatsNode::Array(items) => items.push(key),
            _ => panic!("statistics key is not an array"),
        }
        key
    }

    fn key(&self, map: StatisticsKey, index: usize) -> &str {
        match self.node(map) {
            StatsNode::Map(items) => items
                .keys()
                .nth(index)
                .map(String::as_str)
                .expect("invalid map index"),
            _ => panic!("statistics key is not a map"),
        }
    }

    fn get(&self, map: StatisticsKey, at: &str) -> StatisticsKey {
        match self.node(map) {
            StatsNode::Map(items) => *items.get(at).expect("missing map key"),
            _ => panic!("statistics key is not a map"),
        }
    }

    fn find(&self, map: StatisticsKey, element: &str, out_key: Option<&mut StatisticsKey>) -> bool {
        let found = match self.node(map) {
            StatsNode::Map(items) => items.get(element).copied(),
            _ => panic!("statistics key is not a map"),
        };
        if let (Some(key), Some(out)) = (found, out_key) {
            *out = key;
        }
        found.is_some()
    }

    fn add(&mut self, map: StatisticsKey, name: &str, item_type: StatisticsType) -> StatisticsKey {
        if let StatsNode::Map(items) = self.node(map) {
            if let Some(existing) = items.get(name) {
                return *existing;
            }
        }

        let key = self.push_node(match item_type {
            StatisticsType::Value => StatsNode::Value(0.0),
            StatisticsType::Array => StatsNode::Array(Vec::new()),
            StatisticsType::Map => StatsNode::Map(BTreeMap::new()),
        });
        match self.node_mut(map) {
            StatsNode::Map(items) => {
                items.insert(name.to_owned(), key);
            }
            _ => panic!("statistics key is not a map"),
        }
        key
    }

    fn value(&self, key: StatisticsKey) -> f64 {
        match self.node(key) {
            StatsNode::Value(value) => *value,
            _ => panic!("statistics key is not a value"),
        }
    }

    fn set(&mut self, key: StatisticsKey, value: f64) {
        match self.node_mut(key) {
            StatsNode::Value(slot) => *slot = value,
            _ => panic!("statistics key is not a value"),
        }
    }
}

#[test]
fn compare_program_ignores_line_order_within_each_block() {
    let expected = "a.\nb.  \n0\nc.\nd.\n0\n";
    let actual = "b.\na.\n0\nd.\nc.   \n0\n";

    assert!(compare_program(expected, actual));
}

#[test]
fn compare_program_tracks_duplicate_lines_and_missing_members() {
    let expected = "a.\na.\n0\n";
    let actual = "a.\n0\n";

    assert!(!compare_program(expected, actual));
}

#[test]
fn find_program_matches_across_zero_delimited_sections() {
    let needle = "keep.  \nsecond.\n0\n";
    let actual = "ignore.\n0\nsecond.\nkeep.\n0\nextra.\n";

    assert!(find_program(needle, actual));
}

#[test]
fn find_program_respects_multiplicity() {
    let needle = "x.\nx.\n";
    let actual = "x.\n";

    assert!(!find_program(needle, actual));
}

#[test]
fn add_external_stats_populates_expected_tree() {
    let mut stats = TestStats::default();
    let user_root = stats.add(stats.root(), "user", StatisticsType::Map);

    add_external_stats(&mut stats, user_root);

    let general = stats.get(user_root, "deathCounter");
    assert_eq!(stats.type_of(general), StatisticsType::Map);
    assert_eq!(stats.value(stats.get(general, "total")), 42.0);
    assert_eq!(stats.value(stats.get(general, "chickens")), 712.0);

    let thread = stats.get(general, "thread");
    assert_eq!(stats.type_of(thread), StatisticsType::Array);
    assert_eq!(stats.size(thread), 4);

    let third = stats.at(thread, 2);
    assert_eq!(stats.value(stats.get(third, "total")), 60.0);
    assert_eq!(stats.value(stats.get(third, "feeding cost")), 3.0);

    let animals = stats.get(third, "Animals");
    assert_eq!(stats.value(stats.get(animals, "chicken")), 6.0);
    assert_eq!(stats.value(stats.get(animals, "cows")), 15.0);

    assert_eq!(
        stats.add(user_root, "deathCounter", StatisticsType::Map),
        general
    );
}

#[test]
fn add_external_stats_under_creates_named_root() {
    let mut stats = TestStats::default();

    let user_root = add_external_stats_under(&mut stats, "myRoot");

    assert_eq!(user_root, stats.get(stats.root(), "myRoot"));
    let death_counter = stats.get(user_root, "deathCounter");
    assert_eq!(stats.type_of(death_counter), StatisticsType::Map);
}
