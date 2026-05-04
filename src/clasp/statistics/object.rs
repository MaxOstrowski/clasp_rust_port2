use core::marker::PhantomData;
use core::mem;
use std::panic::panic_any;

use crate::potassco::clingo::{StatisticsKey, StatisticsType};
use crate::potassco::error::Error;

pub type StatisticType = StatisticsType;

pub trait StatisticValue {
    fn to_f64(&self) -> f64;
}

macro_rules! impl_stat_value {
    ($($t:ty),+ $(,)?) => {
        $(
            impl StatisticValue for $t {
                fn to_f64(&self) -> f64 {
                    *self as f64
                }
            }
        )+
    };
}

impl_stat_value!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64);

pub trait StatisticMap {
    fn size(&self) -> u32;
    fn key(&self, index: u32) -> &str;
    fn at<'a>(&'a self, key: &str) -> StatisticObject<'a>;
}

pub trait StatisticArray {
    fn size(&self) -> u32;
    fn at<'a>(&'a self, index: u32) -> StatisticObject<'a>;
}

pub trait StatisticArrayElements {
    type Item;

    fn size(&self) -> u32;
    fn item(&self, index: u32) -> &Self::Item;
}

#[derive(Clone, Copy)]
pub enum StatisticObject<'a> {
    InlineValue(f64),
    ValueRef {
        obj: *const (),
        value: unsafe fn(*const (), usize) -> f64,
        op_id: usize,
        _life: PhantomData<&'a ()>,
    },
    Erased {
        obj: *const (),
        vtab: &'static Vtab,
        op_id: usize,
        _life: PhantomData<&'a ()>,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StatisticObjectTypeId {
    InlineValue,
    ValueRef { value_fn: usize, op_id: usize },
    Erased { vtab: usize, op_id: usize },
}

impl Default for StatisticObject<'_> {
    fn default() -> Self {
        Self::null_value()
    }
}

impl<'a> core::fmt::Debug for StatisticObject<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StatisticObject")
            .field("type_", &self.type_())
            .finish_non_exhaustive()
    }
}

impl PartialEq for StatisticObject<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::InlineValue(lhs), Self::InlineValue(rhs)) => lhs.to_bits() == rhs.to_bits(),
            (
                Self::ValueRef {
                    obj: lhs_obj,
                    value: lhs_value,
                    op_id: lhs_op,
                    ..
                },
                Self::ValueRef {
                    obj: rhs_obj,
                    value: rhs_value,
                    op_id: rhs_op,
                    ..
                },
            ) => {
                lhs_obj == rhs_obj && std::ptr::fn_addr_eq(lhs_value, rhs_value) && lhs_op == rhs_op
            }
            (
                Self::Erased {
                    obj: lhs_obj,
                    vtab: lhs_vtab,
                    op_id: lhs_op,
                    ..
                },
                Self::Erased {
                    obj: rhs_obj,
                    vtab: rhs_vtab,
                    op_id: rhs_op,
                    ..
                },
            ) => lhs_obj == rhs_obj && std::ptr::eq(lhs_vtab, rhs_vtab) && lhs_op == rhs_op,
            _ => false,
        }
    }
}

impl Eq for StatisticObject<'_> {}

impl PartialOrd for StatisticObject<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StatisticObject<'_> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.object() as usize)
            .cmp(&(other.object() as usize))
            .then_with(|| self.type_id().cmp(&other.type_id()))
            .then_with(|| match (*self, *other) {
                (Self::InlineValue(lhs), Self::InlineValue(rhs)) => {
                    lhs.to_bits().cmp(&rhs.to_bits())
                }
                _ => core::cmp::Ordering::Equal,
            })
    }
}

impl<'a> StatisticObject<'a> {
    pub const fn from_f64(value: f64) -> Self {
        Self::InlineValue(value)
    }

    pub fn from_value<T: StatisticValue>(obj: &'a T) -> Self {
        Self::ValueRef {
            obj: core::ptr::from_ref(obj).cast::<()>(),
            value: value_ref::<T>,
            op_id: 0,
            _life: PhantomData,
        }
    }

    pub fn from_mapped_value<T>(obj: &'a T, getter: fn(&T) -> f64) -> Self {
        Self::ValueRef {
            obj: core::ptr::from_ref(obj).cast::<()>(),
            value: mapped_value::<T>,
            op_id: getter as usize,
            _life: PhantomData,
        }
    }

    pub fn map<T: StatisticMap>(obj: &'a T) -> Self {
        Self::Erased {
            obj: core::ptr::from_ref(obj).cast::<()>(),
            vtab: Vtab::map::<T>(),
            op_id: 0,
            _life: PhantomData,
        }
    }

    pub fn array<T: StatisticArray>(obj: &'a T) -> Self {
        Self::Erased {
            obj: core::ptr::from_ref(obj).cast::<()>(),
            vtab: Vtab::array::<T>(),
            op_id: 0,
            _life: PhantomData,
        }
    }

    pub fn array_with<T: StatisticArrayElements>(
        obj: &'a T,
        getter: for<'b> fn(&'b T::Item) -> StatisticObject<'b>,
    ) -> Self {
        Self::Erased {
            obj: core::ptr::from_ref(obj).cast::<()>(),
            vtab: Vtab::mapped_array::<T>(),
            op_id: getter as usize,
            _life: PhantomData,
        }
    }

    pub const fn type_(&self) -> StatisticType {
        match self {
            Self::InlineValue(_) | Self::ValueRef { .. } => StatisticType::Value,
            Self::Erased { vtab, .. } => vtab.type_,
        }
    }

    pub const fn r#type(&self) -> StatisticType {
        self.type_()
    }

    pub fn size(&self) -> u32 {
        match self {
            Self::InlineValue(_) | Self::ValueRef { .. } => 0,
            Self::Erased { obj, vtab, .. } => unsafe { (vtab.size)(*obj) },
        }
    }

    pub fn key(&self, index: u32) -> &'a str {
        match self {
            Self::Erased { obj, vtab, .. } if vtab.type_ == StatisticType::Map => unsafe {
                mem::transmute::<&str, &'a str>((vtab.key)(*obj, index))
            },
            _ => panic_type(StatisticType::Map, self.type_()),
        }
    }

    pub fn at(&self, key: &str) -> StatisticObject<'a> {
        match self {
            Self::Erased { obj, vtab, .. } if vtab.type_ == StatisticType::Map => unsafe {
                let child = (vtab.at_map)(*obj, key);
                mem::transmute::<StatisticObject<'static>, StatisticObject<'a>>(child)
            },
            _ => panic_type(StatisticType::Map, self.type_()),
        }
    }

    pub fn index(&self, index: u32) -> StatisticObject<'a> {
        match self {
            Self::Erased {
                obj, vtab, op_id, ..
            } if vtab.type_ == StatisticType::Array => unsafe {
                let child = (vtab.at_arr)(*obj, index, *op_id);
                mem::transmute::<StatisticObject<'static>, StatisticObject<'a>>(child)
            },
            _ => panic_type(StatisticType::Array, self.type_()),
        }
    }

    pub fn at_index(&self, index: u32) -> StatisticObject<'a> {
        self.index(index)
    }

    pub fn value(&self) -> f64 {
        match self {
            Self::InlineValue(value) => *value,
            Self::ValueRef {
                obj, value, op_id, ..
            } => unsafe { (value)(*obj, *op_id) },
            Self::Erased { .. } => panic_type(StatisticType::Value, self.type_()),
        }
    }

    pub const fn object(&self) -> *const () {
        match self {
            Self::InlineValue(_) => core::ptr::null(),
            Self::ValueRef { obj, .. } | Self::Erased { obj, .. } => *obj,
        }
    }

    fn null_value() -> Self {
        Self::ValueRef {
            obj: core::ptr::null(),
            value: null_value,
            op_id: 0,
            _life: PhantomData,
        }
    }

    pub(crate) fn external_key(&self) -> super::store::ExternalObjectKey {
        match *self {
            Self::InlineValue(value) => {
                super::store::ExternalObjectKey::InlineValue(value.to_bits())
            }
            Self::ValueRef {
                obj, value, op_id, ..
            } => super::store::ExternalObjectKey::ValueRef {
                obj: obj as usize,
                value: value as usize,
                op_id,
            },
            Self::Erased {
                obj, vtab, op_id, ..
            } => super::store::ExternalObjectKey::Erased {
                obj: obj as usize,
                vtab: vtab as *const Vtab as usize,
                op_id,
            },
        }
    }

    pub fn type_id(&self) -> StatisticObjectTypeId {
        match *self {
            Self::InlineValue(_) => StatisticObjectTypeId::InlineValue,
            Self::ValueRef { value, op_id, .. } => StatisticObjectTypeId::ValueRef {
                value_fn: value as usize,
                op_id,
            },
            Self::Erased { vtab, op_id, .. } => StatisticObjectTypeId::Erased {
                vtab: vtab as *const Vtab as usize,
                op_id,
            },
        }
    }

    pub fn eq_type_id(&self, other: &Self) -> bool {
        self.type_id() == other.type_id()
    }
}

#[doc(hidden)]
pub struct Vtab {
    type_: StatisticType,
    size: unsafe fn(*const ()) -> u32,
    key: unsafe fn(*const (), u32) -> &'static str,
    at_map: unsafe fn(*const (), &str) -> StatisticObject<'static>,
    at_arr: unsafe fn(*const (), u32, usize) -> StatisticObject<'static>,
}

impl Vtab {
    const fn new(type_: StatisticType) -> Self {
        Self {
            type_,
            size: panic_size,
            key: panic_key_vtab,
            at_map: panic_at_map,
            at_arr: panic_at_arr,
        }
    }

    fn map<T: StatisticMap>() -> &'static Self {
        &MapVtab::<T>::VTAB
    }

    fn array<T: StatisticArray>() -> &'static Self {
        &ArrayVtab::<T>::VTAB
    }

    fn mapped_array<T: StatisticArrayElements>() -> &'static Self {
        &MappedArrayVtab::<T>::VTAB
    }
}

unsafe fn value_ref<T: StatisticValue>(obj: *const (), _op_id: usize) -> f64 {
    unsafe { (*(obj.cast::<T>())).to_f64() }
}

unsafe fn mapped_value<T>(obj: *const (), op_id: usize) -> f64 {
    let getter = unsafe { mem::transmute::<usize, fn(&T) -> f64>(op_id) };
    getter(unsafe { &*(obj.cast::<T>()) })
}

unsafe fn null_value(_obj: *const (), _op_id: usize) -> f64 {
    0.0
}

unsafe fn v_map_size<T: StatisticMap>(obj: *const ()) -> u32 {
    unsafe { (*(obj.cast::<T>())).size() }
}

unsafe fn v_map_key<T: StatisticMap>(obj: *const (), index: u32) -> &'static str {
    let key = unsafe { (*(obj.cast::<T>())).key(index) };
    unsafe { mem::transmute::<&str, &'static str>(key) }
}

unsafe fn v_map_at<T: StatisticMap>(obj: *const (), key: &str) -> StatisticObject<'static> {
    let child = unsafe { (*(obj.cast::<T>())).at(key) };
    unsafe { mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(child) }
}

unsafe fn v_arr_size<T: StatisticArray>(obj: *const ()) -> u32 {
    unsafe { (*(obj.cast::<T>())).size() }
}

unsafe fn v_arr_at<T: StatisticArray>(
    obj: *const (),
    index: u32,
    _op_id: usize,
) -> StatisticObject<'static> {
    let child = unsafe { (*(obj.cast::<T>())).at(index) };
    unsafe { mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(child) }
}

type ArrayGetter<T> = for<'a> fn(&'a <T as StatisticArrayElements>::Item) -> StatisticObject<'a>;

unsafe fn v_mapped_arr_size<T: StatisticArrayElements>(obj: *const ()) -> u32 {
    unsafe { (*(obj.cast::<T>())).size() }
}

unsafe fn v_mapped_arr_at<T: StatisticArrayElements>(
    obj: *const (),
    index: u32,
    op_id: usize,
) -> StatisticObject<'static> {
    let getter = unsafe { mem::transmute::<usize, ArrayGetter<T>>(op_id) };
    let child = getter(unsafe { (*(obj.cast::<T>())).item(index) });
    unsafe { mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(child) }
}

struct MapVtab<T>(PhantomData<T>);
impl<T: StatisticMap> MapVtab<T> {
    const VTAB: Vtab = {
        let mut v = Vtab::new(StatisticType::Map);
        v.size = v_map_size::<T>;
        v.key = v_map_key::<T>;
        v.at_map = v_map_at::<T>;
        v
    };
}

struct ArrayVtab<T>(PhantomData<T>);
impl<T: StatisticArray> ArrayVtab<T> {
    const VTAB: Vtab = {
        let mut v = Vtab::new(StatisticType::Array);
        v.size = v_arr_size::<T>;
        v.at_arr = v_arr_at::<T>;
        v
    };
}

struct MappedArrayVtab<T>(PhantomData<T>);
impl<T: StatisticArrayElements> MappedArrayVtab<T> {
    const VTAB: Vtab = {
        let mut v = Vtab::new(StatisticType::Array);
        v.size = v_mapped_arr_size::<T>;
        v.at_arr = v_mapped_arr_at::<T>;
        v
    };
}

pub(crate) fn parse_array_index(value: &str) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse::<u32>().ok()
}

fn stat_type_name(value: StatisticType) -> &'static str {
    match value {
        StatisticType::Value => "value",
        StatisticType::Array => "array",
        StatisticType::Map => "map",
    }
}

pub(crate) fn panic_type(expected: StatisticType, got: StatisticType) -> ! {
    panic_any(Error::InvalidArgument(format!(
        "bad stats access: '{}' expected but got '{}'",
        stat_type_name(expected),
        stat_type_name(got)
    )))
}

pub(crate) fn panic_invalid_key(key: StatisticsKey) -> ! {
    panic_any(Error::InvalidArgument(format!(
        "bad stats access: invalid key '{key}'"
    )))
}

pub(crate) fn panic_path(path: &str, at: &str) -> ! {
    if path.is_empty() || at.is_empty() {
        let target = if at.is_empty() { path } else { at };
        panic_any(Error::OutOfRange(format!(
            "bad stats access: invalid key '{target}'"
        )));
    }
    panic_any(Error::OutOfRange(format!(
        "bad stats access: invalid key '{at}' in path '{path}'"
    )))
}

pub(crate) fn panic_write(key: StatisticsKey, item_type: StatisticType) -> ! {
    panic_any(Error::InvalidArgument(format!(
        "bad stats access: key '{key}' is not a writable {}",
        stat_type_name(item_type)
    )))
}

pub(crate) fn panic_range(index: usize, size: usize) -> ! {
    panic_any(Error::OutOfRange(format!(
        "bad stats access: index '{index}' is out of range for object of size '{size}'"
    )))
}

pub(crate) fn panic_not_accessible() -> ! {
    panic_any(Error::InvalidArgument(
        "statistics not (yet) accessible".to_owned(),
    ))
}

unsafe fn panic_size(_obj: *const ()) -> u32 {
    panic_type(StatisticType::Array, StatisticType::Value)
}

unsafe fn panic_key_vtab(_obj: *const (), _index: u32) -> &'static str {
    panic_type(StatisticType::Map, StatisticType::Value)
}

unsafe fn panic_at_map(_obj: *const (), _key: &str) -> StatisticObject<'static> {
    panic_type(StatisticType::Map, StatisticType::Value)
}

unsafe fn panic_at_arr(_obj: *const (), _index: u32, _op_id: usize) -> StatisticObject<'static> {
    panic_type(StatisticType::Array, StatisticType::Value)
}
