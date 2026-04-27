use core::mem;
use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};

use crate::clasp::claspfwd::Asp;
use crate::clasp::shared_context::ProblemStats;
use crate::clasp::solver_types::SolverStats;
use crate::potassco::clingo::{AbstractStatistics, StatisticsKey, StatisticsType};
use crate::potassco::error::Error;

use super::object::{
    StatisticArray, StatisticMap, StatisticObject, StatisticType, panic_invalid_key,
    panic_not_accessible, panic_path, panic_range, panic_type, panic_write, parse_array_index,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ExternalObjectKey {
    InlineValue(u64),
    ValueRef {
        obj: usize,
        value: usize,
        op_id: usize,
    },
    Erased {
        obj: usize,
        vtab: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyType {
    Value = 0,
    Array = 1,
    Map = 2,
    Ext = 3,
}

const KEY_SHIFT: u64 = 62;

const fn make_key(key_type: KeyType, index: usize) -> StatisticsKey {
    ((key_type as StatisticsKey) << KEY_SHIFT) | index as StatisticsKey
}

const fn key_type(key: StatisticsKey) -> KeyType {
    match key >> KEY_SHIFT {
        0 => KeyType::Value,
        1 => KeyType::Array,
        2 => KeyType::Map,
        3 => KeyType::Ext,
        _ => unreachable!(),
    }
}

const fn key_index(key: StatisticsKey) -> usize {
    (key & ((1u64 << KEY_SHIFT) - 1)) as usize
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Enter,
    Leave,
}

pub trait StatsVisitor {
    fn visit_generator(&mut self, _op: Operation) -> bool {
        true
    }

    fn visit_threads(&mut self, _op: Operation) -> bool {
        true
    }

    fn visit_tester(&mut self, _op: Operation) -> bool {
        true
    }

    fn visit_hccs(&mut self, _op: Operation) -> bool {
        true
    }

    fn visit_thread(&mut self, _thread_id: u32, stats: &SolverStats) {
        self.visit_solver_stats(stats);
    }

    fn visit_hcc(&mut self, _hcc_id: u32, problem: &ProblemStats, solver: &SolverStats) {
        self.visit_problem_stats(problem);
        self.visit_solver_stats(solver);
    }

    fn visit_logic_program_stats(&mut self, stats: &Asp::LpStats);
    fn visit_problem_stats(&mut self, stats: &ProblemStats);
    fn visit_solver_stats(&mut self, stats: &SolverStats);
    fn visit_external_stats(&mut self, stats: StatisticObject<'_>);
}

#[derive(Debug)]
pub struct ClaspStatistics {
    inner: Box<ClaspStatisticsInner>,
}

impl Default for ClaspStatistics {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaspStatistics {
    pub fn new() -> Self {
        let mut inner = Box::new(ClaspStatisticsInner::new());
        let owner = inner.as_ref() as *const ClaspStatisticsInner;
        inner.maps.push(WritableMap::new(owner));
        Self { inner }
    }

    pub fn add_object(
        &mut self,
        map: StatisticsKey,
        name: &str,
        object: StatisticObject<'_>,
        skip_check: bool,
    ) -> StatisticsKey {
        self.inner.add_map_object(map, name, object, skip_check)
    }

    pub fn visit_external(&self, name: &str, visitor: &mut dyn StatsVisitor) -> bool {
        if let Some(key) = self.inner.root_map().find(name) {
            visitor.visit_external_stats(self.inner.object_for_key(key));
            true
        } else {
            false
        }
    }

    pub fn freeze(&mut self, frozen: bool) {
        self.inner.frozen = frozen;
    }

    #[allow(non_snake_case)]
    pub fn addObject(
        &mut self,
        map: StatisticsKey,
        name: &str,
        object: StatisticObject<'_>,
        skip_check: bool,
    ) -> StatisticsKey {
        self.add_object(map, name, object, skip_check)
    }

    #[allow(non_snake_case)]
    pub fn visitExternal(&self, name: &str, visitor: &mut dyn StatsVisitor) -> bool {
        self.visit_external(name, visitor)
    }

    pub fn root(&self) -> StatisticsKey {
        make_key(KeyType::Map, 0)
    }

    pub fn r#type(&self, key: StatisticsKey) -> StatisticType {
        self.type_of(key)
    }

    pub fn type_of(&self, key: StatisticsKey) -> StatisticType {
        self.inner.object_for_key(key).type_()
    }

    pub fn size(&self, key: StatisticsKey) -> usize {
        self.inner.object_for_key(key).size() as usize
    }

    pub fn writable(&self, key: StatisticsKey) -> bool {
        let _ = self.inner.object_for_key(key);
        key_type(key) != KeyType::Ext
    }

    pub fn at(&self, array: StatisticsKey, index: usize) -> StatisticsKey {
        self.inner.child_key_at(array, index)
    }

    pub fn push(&mut self, array: StatisticsKey, item_type: StatisticType) -> StatisticsKey {
        self.inner.push_array(array, item_type)
    }

    pub fn key(&self, map: StatisticsKey, index: usize) -> &str {
        let object = self.inner.object_for_key(map);
        if object.type_() != StatisticType::Map {
            panic_type(StatisticType::Map, object.type_());
        }
        if index >= object.size() as usize {
            panic_range(index, object.size() as usize);
        }
        object.key(index as u32)
    }

    pub fn get(&self, map: StatisticsKey, path: &str) -> StatisticsKey {
        self.inner.child_key_get(map, path)
    }

    pub fn find(
        &self,
        map: StatisticsKey,
        element: &str,
        out_key: Option<&mut StatisticsKey>,
    ) -> bool {
        match catch_unwind(AssertUnwindSafe(|| self.get(map, element))) {
            Ok(key) => {
                if let Some(out) = out_key {
                    *out = key;
                }
                true
            }
            Err(_) => false,
        }
    }

    pub fn add(
        &mut self,
        map: StatisticsKey,
        name: &str,
        item_type: StatisticType,
    ) -> StatisticsKey {
        self.inner.add_map(map, name, item_type)
    }

    pub fn value(&self, key: StatisticsKey) -> f64 {
        self.inner.object_for_key(key).value()
    }

    pub fn set(&mut self, key: StatisticsKey, value: f64) {
        self.inner.set_value(key, value);
    }
}

impl AbstractStatistics for ClaspStatistics {
    fn root(&self) -> StatisticsKey {
        self.root()
    }

    fn type_of(&self, key: StatisticsKey) -> StatisticsType {
        self.type_of(key)
    }

    fn size(&self, key: StatisticsKey) -> usize {
        self.size(key)
    }

    fn writable(&self, key: StatisticsKey) -> bool {
        self.writable(key)
    }

    fn at(&self, array: StatisticsKey, index: usize) -> StatisticsKey {
        self.at(array, index)
    }

    fn push(&mut self, array: StatisticsKey, item_type: StatisticsType) -> StatisticsKey {
        self.push(array, item_type)
    }

    fn key(&self, map: StatisticsKey, index: usize) -> &str {
        self.key(map, index)
    }

    fn get(&self, map: StatisticsKey, at: &str) -> StatisticsKey {
        self.get(map, at)
    }

    fn find(&self, map: StatisticsKey, element: &str, out_key: Option<&mut StatisticsKey>) -> bool {
        self.find(map, element, out_key)
    }

    fn add(&mut self, map: StatisticsKey, name: &str, item_type: StatisticsType) -> StatisticsKey {
        self.add(map, name, item_type)
    }

    fn value(&self, key: StatisticsKey) -> f64 {
        self.value(key)
    }

    fn set(&mut self, key: StatisticsKey, value: f64) {
        self.set(key, value);
    }
}

#[derive(Debug)]
struct WritableMap {
    owner: *const ClaspStatisticsInner,
    entries: Vec<(String, StatisticsKey)>,
}

impl WritableMap {
    fn new(owner: *const ClaspStatisticsInner) -> Self {
        Self {
            owner,
            entries: Vec::new(),
        }
    }

    fn find(&self, name: &str) -> Option<StatisticsKey> {
        self.entries
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, child)| *child)
    }

    fn child(&self, name: &str) -> StatisticsKey {
        self.find(name).unwrap_or_else(|| {
            panic_any(Error::OutOfRange(format!(
                "WritableMap::at with key '{name}'"
            )))
        })
    }

    fn add(&mut self, name: &str, key: StatisticsKey) {
        self.entries.push((name.to_owned(), key));
    }
}

impl StatisticMap for WritableMap {
    fn size(&self) -> u32 {
        self.entries.len() as u32
    }

    fn key(&self, index: u32) -> &str {
        self.entries[index as usize].0.as_str()
    }

    fn at<'a>(&'a self, key: &str) -> StatisticObject<'a> {
        let child = self.child(key);
        let object = unsafe { (*self.owner).object_for_key(child) };
        unsafe { mem::transmute::<StatisticObject<'_>, StatisticObject<'a>>(object) }
    }
}

#[derive(Debug)]
struct WritableArray {
    owner: *const ClaspStatisticsInner,
    entries: Vec<StatisticsKey>,
}

impl WritableArray {
    fn new(owner: *const ClaspStatisticsInner) -> Self {
        Self {
            owner,
            entries: Vec::new(),
        }
    }

    fn child(&self, index: u32) -> StatisticsKey {
        self.entries[index as usize]
    }

    fn add(&mut self, key: StatisticsKey) {
        self.entries.push(key);
    }
}

impl StatisticArray for WritableArray {
    fn size(&self) -> u32 {
        self.entries.len() as u32
    }

    fn at<'a>(&'a self, index: u32) -> StatisticObject<'a> {
        let child = self.child(index);
        let object = unsafe { (*self.owner).object_for_key(child) };
        unsafe { mem::transmute::<StatisticObject<'_>, StatisticObject<'a>>(object) }
    }
}

#[derive(Debug)]
struct ClaspStatisticsInner {
    ext: RefCell<Vec<StatisticObject<'static>>>,
    maps: Vec<WritableMap>,
    arrays: Vec<WritableArray>,
    values: Vec<f64>,
    ext_index: RefCell<HashMap<ExternalObjectKey, StatisticsKey>>,
    frozen: bool,
}

impl ClaspStatisticsInner {
    fn new() -> Self {
        Self {
            ext: RefCell::new(Vec::new()),
            maps: Vec::new(),
            arrays: Vec::new(),
            values: Vec::new(),
            ext_index: RefCell::new(HashMap::new()),
            frozen: false,
        }
    }

    fn root_map(&self) -> &WritableMap {
        &self.maps[0]
    }

    fn object_for_key<'a>(&'a self, key: StatisticsKey) -> StatisticObject<'a> {
        let index = key_index(key);
        match key_type(key) {
            KeyType::Value => {
                let value = self
                    .values
                    .get(index)
                    .unwrap_or_else(|| panic_invalid_key(key));
                StatisticObject::from_value(value)
            }
            KeyType::Array => {
                let array = self
                    .arrays
                    .get(index)
                    .unwrap_or_else(|| panic_invalid_key(key));
                StatisticObject::array(array)
            }
            KeyType::Map => {
                let map = self
                    .maps
                    .get(index)
                    .unwrap_or_else(|| panic_invalid_key(key));
                StatisticObject::map(map)
            }
            KeyType::Ext => {
                if self.frozen {
                    panic_not_accessible();
                }
                let object = self
                    .ext
                    .borrow()
                    .get(index)
                    .copied()
                    .unwrap_or_else(|| panic_invalid_key(key));
                unsafe { mem::transmute::<StatisticObject<'static>, StatisticObject<'a>>(object) }
            }
        }
    }

    fn add_writable(&mut self, item_type: StatisticType) -> StatisticsKey {
        let owner = self as *const ClaspStatisticsInner;
        match item_type {
            StatisticType::Value => {
                self.values.push(0.0);
                make_key(KeyType::Value, self.values.len() - 1)
            }
            StatisticType::Array => {
                self.arrays.push(WritableArray::new(owner));
                make_key(KeyType::Array, self.arrays.len() - 1)
            }
            StatisticType::Map => {
                self.maps.push(WritableMap::new(owner));
                make_key(KeyType::Map, self.maps.len() - 1)
            }
        }
    }

    fn ensure_writable(&self, key: StatisticsKey, item_type: StatisticType) -> usize {
        let object = self.object_for_key(key);
        if key_type(key) != KeyType::Ext && object.type_() == item_type {
            key_index(key)
        } else {
            panic_write(key, item_type);
        }
    }

    fn add_external(&self, object: StatisticObject<'_>, skip_mapping: bool) -> StatisticsKey {
        let object =
            unsafe { mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(object) };
        let external_key = object.external_key();
        if !skip_mapping {
            if let Some(existing) = self.ext_index.borrow().get(&external_key).copied() {
                return existing;
            }
        }

        let key = {
            let mut ext = self.ext.borrow_mut();
            let key = make_key(KeyType::Ext, ext.len());
            ext.push(object);
            key
        };
        if !skip_mapping {
            self.ext_index.borrow_mut().insert(external_key, key);
        }
        key
    }

    fn add_map(
        &mut self,
        map: StatisticsKey,
        name: &str,
        item_type: StatisticType,
    ) -> StatisticsKey {
        let index = self.ensure_writable(map, StatisticType::Map);
        if let Some(existing) = self.maps[index].find(name) {
            let _ = self.ensure_writable(existing, item_type);
            return existing;
        }

        let child = self.add_writable(item_type);
        self.maps[index].add(name, child);
        child
    }

    fn add_map_object(
        &mut self,
        map: StatisticsKey,
        name: &str,
        object: StatisticObject<'_>,
        skip_check: bool,
    ) -> StatisticsKey {
        let index = self.ensure_writable(map, StatisticType::Map);
        if !skip_check {
            if let Some(existing) = self.maps[index].find(name) {
                if self.object_for_key(existing) == object {
                    return existing;
                }
                panic_any(Error::InvalidArgument(format!(
                    "unexpected object for key '{name}'"
                )));
            }
        }

        let child = self.add_external(object, true);
        self.maps[index].add(name, child);
        child
    }

    fn set_value(&mut self, key: StatisticsKey, value: f64) {
        let index = self.ensure_writable(key, StatisticType::Value);
        self.values[index] = value;
    }

    fn push_array(&mut self, array: StatisticsKey, item_type: StatisticType) -> StatisticsKey {
        let index = self.ensure_writable(array, StatisticType::Array);
        let child = self.add_writable(item_type);
        self.arrays[index].add(child);
        child
    }

    fn child_key_at(&self, array: StatisticsKey, index: usize) -> StatisticsKey {
        let object = self.object_for_key(array);
        if object.type_() != StatisticType::Array {
            panic_type(StatisticType::Array, object.type_());
        }
        if index >= object.size() as usize {
            panic_range(index, object.size() as usize);
        }

        match key_type(array) {
            KeyType::Array => self.arrays[key_index(array)].child(index as u32),
            KeyType::Ext => {
                let child = unsafe {
                    mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(
                        object.index(index as u32),
                    )
                };
                self.add_external(child, false)
            }
            _ => unreachable!(),
        }
    }

    fn child_key_get(&self, map: StatisticsKey, path: &str) -> StatisticsKey {
        let mut current_key = map;
        let mut current_object = unsafe {
            mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(
                self.object_for_key(map),
            )
        };
        let has_key = key_type(map) != KeyType::Ext;
        let mut position = 0usize;

        while position < path.len() {
            let rest = &path[position..];
            let offset = rest.find('.').unwrap_or(rest.len());
            let segment = &rest[..offset];
            let consumed = position + offset;

            match current_object.type_() {
                StatisticType::Map => {
                    if has_key {
                        let map_index = key_index(current_key);
                        let child = self.maps[map_index]
                            .find(segment)
                            .unwrap_or_else(|| panic_path(&path[..consumed], segment));
                        current_key = child;
                        current_object = unsafe {
                            mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(
                                self.object_for_key(child),
                            )
                        };
                    } else {
                        let next = catch_unwind(AssertUnwindSafe(|| current_object.at(segment)))
                            .unwrap_or_else(|_| panic_path(&path[..consumed], segment));
                        current_object = unsafe {
                            mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(next)
                        };
                    }
                }
                StatisticType::Array => {
                    let index = parse_array_index(segment)
                        .filter(|array_index| {
                            (*array_index as usize) < current_object.size() as usize
                        })
                        .unwrap_or_else(|| panic_path(&path[..consumed], segment));
                    if has_key {
                        current_key = self.arrays[key_index(current_key)].child(index);
                        current_object = unsafe {
                            mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(
                                self.object_for_key(current_key),
                            )
                        };
                    } else {
                        current_object = unsafe {
                            mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(
                                current_object.index(index),
                            )
                        };
                    }
                }
                StatisticType::Value => panic_path(&path[..consumed], segment),
            }

            position = if consumed == path.len() {
                consumed
            } else {
                consumed + 1
            };
        }

        if has_key {
            current_key
        } else {
            self.add_external(current_object, false)
        }
    }
}
