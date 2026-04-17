//! Partial Rust port of original_clasp/tests/lpcompare.h.

use rust_clasp::potassco::clingo::{AbstractStatistics, StatisticsKey, StatisticsType};

fn trim_r(line: &str) -> &str {
    line.trim_end_matches(' ')
}

pub fn compare_program(expected: &str, actual: &str) -> bool {
    let mut expected_lines = expected.lines();
    let mut actual_lines = actual.lines();

    loop {
        let mut block = Vec::new();
        let mut expected_progress = false;

        for line in expected_lines.by_ref() {
            expected_progress = true;
            if line == "0" {
                break;
            }
            block.push(trim_r(line).to_owned());
        }

        let mut actual_progress = false;
        for line in actual_lines.by_ref() {
            actual_progress = true;
            if line == "0" {
                break;
            }
            let trimmed = trim_r(line);
            let Some(index) = block.iter().position(|candidate| candidate == trimmed) else {
                return false;
            };
            block.remove(index);
        }

        if !block.is_empty() {
            return false;
        }
        if !expected_progress && !actual_progress {
            return true;
        }
    }
}

pub fn find_program(needle: &str, actual: &str) -> bool {
    let mut remaining: Vec<String> = needle
        .lines()
        .filter(|line| *line != "0")
        .map(|line| trim_r(line).to_owned())
        .collect();

    for line in actual.lines() {
        if remaining.is_empty() {
            break;
        }
        if line == "0" {
            continue;
        }
        let trimmed = trim_r(line);
        if let Some(index) = remaining.iter().position(|candidate| candidate == trimmed) {
            remaining.remove(index);
        }
    }

    remaining.is_empty()
}

pub fn add_external_stats(stats: &mut dyn AbstractStatistics, user_root: StatisticsKey) {
    let general = stats.add(user_root, "deathCounter", StatisticsType::Map);
    assert_eq!(stats.get(user_root, "deathCounter"), general);
    assert_eq!(stats.type_of(general), StatisticsType::Map);

    let mut value = stats.add(general, "total", StatisticsType::Value);
    stats.set(value, 42.0);
    value = stats.add(general, "chickens", StatisticsType::Value);
    stats.set(value, 712.0);

    let array = stats.add(general, "thread", StatisticsType::Array);
    assert_eq!(stats.get(general, "thread"), array);
    assert_eq!(stats.type_of(array), StatisticsType::Array);
    assert_eq!(stats.size(array), 0);

    for thread_index in 0..4 {
        let entry = stats.push(array, StatisticsType::Map);
        value = stats.add(entry, "total", StatisticsType::Value);
        stats.set(value, 20.0 * (thread_index + 1) as f64);

        let animals = stats.add(entry, "Animals", StatisticsType::Map);
        value = stats.add(animals, "chicken", StatisticsType::Value);
        stats.set(value, 2.0 * (thread_index + 1) as f64);
        value = stats.add(animals, "cows", StatisticsType::Value);
        stats.set(value, 5.0 * (thread_index + 1) as f64);

        value = stats.add(entry, "feeding cost", StatisticsType::Value);
        stats.set(value, (thread_index + 1) as f64);
    }

    assert_eq!(
        stats.add(user_root, "deathCounter", StatisticsType::Map),
        general
    );
}

pub fn add_external_stats_under(stats: &mut dyn AbstractStatistics, root: &str) -> StatisticsKey {
    let user_root = stats.add(stats.root(), root, StatisticsType::Map);
    add_external_stats(stats, user_root);
    user_root
}
