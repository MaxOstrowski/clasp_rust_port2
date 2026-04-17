//! Partial Rust port of `original_clasp/clasp/statistics.h`.
//!
//! This module provides a *solver-agnostic* statistics object model similar to the
//! upstream C++ `StatisticObject`.
//!
//! The core design constraint is that this module must not depend on solver/runtime
//! types (e.g., `SolverStats`). Instead, those types implement the small trait
//! surfaces (`StatisticMap` / `StatisticArray`) and can then be exported through
//! `StatisticObject`.

use core::marker::PhantomData;
use core::mem;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatisticType {
    Value,
    Map,
    Array,
}

pub trait StatisticValue {
    fn to_f64(&self) -> f64;
}

macro_rules! impl_stat_value {
	($($t:ty),+ $(,)?) => {
		$(
			impl StatisticValue for $t {
				fn to_f64(&self) -> f64 { *self as f64 }
			}
		)+
	};
}

impl_stat_value!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64);

pub trait StatisticMap {
    fn size(&self) -> u32;
    fn key(&self, index: u32) -> &'static str;
    fn at<'a>(&'a self, key: &str) -> StatisticObject<'a>;
}

pub trait StatisticArray {
    fn size(&self) -> u32;
    fn at<'a>(&'a self, index: u32) -> StatisticObject<'a>;
}

#[derive(Clone, Copy)]
pub enum StatisticObject<'a> {
    InlineValue(f64),
    Erased {
        obj: *const (),
        vtab: &'static Vtab,
        _life: PhantomData<&'a ()>,
    },
}

impl<'a> core::fmt::Debug for StatisticObject<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StatisticObject")
            .field("type_", &self.type_())
            .finish_non_exhaustive()
    }
}

impl<'a> StatisticObject<'a> {
    pub const fn from_f64(value: f64) -> Self {
        Self::InlineValue(value)
    }

    pub fn from_value<T: StatisticValue>(obj: &'a T) -> Self {
        let obj_ptr = core::ptr::from_ref(obj).cast::<()>();
        Self::Erased {
            obj: obj_ptr,
            vtab: Vtab::value::<T>(),
            _life: PhantomData,
        }
    }

    pub fn map<T: StatisticMap>(obj: &'a T) -> Self {
        let obj_ptr = core::ptr::from_ref(obj).cast::<()>();
        Self::Erased {
            obj: obj_ptr,
            vtab: Vtab::map::<T>(),
            _life: PhantomData,
        }
    }

    pub fn array<T: StatisticArray>(obj: &'a T) -> Self {
        let obj_ptr = core::ptr::from_ref(obj).cast::<()>();
        Self::Erased {
            obj: obj_ptr,
            vtab: Vtab::array::<T>(),
            _life: PhantomData,
        }
    }

    pub const fn type_(&self) -> StatisticType {
        match self {
            Self::InlineValue(_) => StatisticType::Value,
            Self::Erased { vtab, .. } => vtab.type_,
        }
    }

    pub fn size(&self) -> u32 {
        match self {
            Self::InlineValue(_) => 0,
            Self::Erased { obj, vtab, .. } => unsafe { (vtab.size)(*obj) },
        }
    }

    pub fn key(&self, index: u32) -> &'static str {
        match self {
            Self::InlineValue(_) => panic!("StatisticObject::key called on value object"),
            Self::Erased { obj, vtab, .. } => unsafe { (vtab.key)(*obj, index) },
        }
    }

    pub fn at(&self, key: &str) -> StatisticObject<'a> {
        match self {
            Self::InlineValue(_) => panic!("StatisticObject::at called on value object"),
            Self::Erased { obj, vtab, .. } => unsafe {
                let child: StatisticObject<'static> = (vtab.at_map)(*obj, key);
                mem::transmute::<StatisticObject<'static>, StatisticObject<'a>>(child)
            },
        }
    }

    pub fn index(&self, index: u32) -> StatisticObject<'a> {
        match self {
            Self::InlineValue(_) => panic!("StatisticObject::index called on value object"),
            Self::Erased { obj, vtab, .. } => unsafe {
                let child: StatisticObject<'static> = (vtab.at_arr)(*obj, index);
                mem::transmute::<StatisticObject<'static>, StatisticObject<'a>>(child)
            },
        }
    }

    pub fn value_as_f64(&self) -> f64 {
        match self {
            Self::InlineValue(v) => *v,
            Self::Erased { obj, vtab, .. } => unsafe { (vtab.value)(*obj) },
        }
    }

    pub fn value(&self) -> f64 {
        self.value_as_f64()
    }
}

#[doc(hidden)]
pub struct Vtab {
    type_: StatisticType,
    size: unsafe fn(*const ()) -> u32,
    key: unsafe fn(*const (), u32) -> &'static str,
    at_map: unsafe fn(*const (), &str) -> StatisticObject<'static>,
    at_arr: unsafe fn(*const (), u32) -> StatisticObject<'static>,
    value: unsafe fn(*const ()) -> f64,
}

impl Vtab {
    const fn new(type_: StatisticType) -> Self {
        Self {
            type_,
            size: panic_size,
            key: panic_key,
            at_map: panic_at_map,
            at_arr: panic_at_arr,
            value: panic_value,
        }
    }

    fn value<T: StatisticValue>() -> &'static Self {
        &ValueVtab::<T>::VTAB
    }

    fn map<T: StatisticMap>() -> &'static Self {
        &MapVtab::<T>::VTAB
    }

    fn array<T: StatisticArray>() -> &'static Self {
        &ArrayVtab::<T>::VTAB
    }
}

unsafe fn v_value<T: StatisticValue>(obj: *const ()) -> f64 {
    let obj = unsafe { &*(obj.cast::<T>()) };
    obj.to_f64()
}

unsafe fn v_map_size<T: StatisticMap>(obj: *const ()) -> u32 {
    unsafe { (*(obj.cast::<T>())).size() }
}

unsafe fn v_map_key<T: StatisticMap>(obj: *const (), index: u32) -> &'static str {
    unsafe { (*(obj.cast::<T>())).key(index) }
}

unsafe fn v_map_at<T: StatisticMap>(obj: *const (), key: &str) -> StatisticObject<'static> {
    let child = unsafe { (*(obj.cast::<T>())).at(key) };
    unsafe { mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(child) }
}

unsafe fn v_arr_size<T: StatisticArray>(obj: *const ()) -> u32 {
    unsafe { (*(obj.cast::<T>())).size() }
}

unsafe fn v_arr_at<T: StatisticArray>(obj: *const (), index: u32) -> StatisticObject<'static> {
    let child = unsafe { (*(obj.cast::<T>())).at(index) };
    unsafe { mem::transmute::<StatisticObject<'_>, StatisticObject<'static>>(child) }
}

struct ValueVtab<T>(PhantomData<T>);
impl<T: StatisticValue> ValueVtab<T> {
    const VTAB: Vtab = {
        let mut v = Vtab::new(StatisticType::Value);
        v.value = v_value::<T>;
        v
    };
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

unsafe fn panic_size(_obj: *const ()) -> u32 {
    panic!("StatisticObject::size called on non-composite object")
}

unsafe fn panic_key(_obj: *const (), _index: u32) -> &'static str {
    panic!("StatisticObject::key called on non-map object")
}

unsafe fn panic_at_map(_obj: *const (), _key: &str) -> StatisticObject<'static> {
    panic!("StatisticObject::at called on non-map object")
}

unsafe fn panic_at_arr(_obj: *const (), _index: u32) -> StatisticObject<'static> {
    panic!("StatisticObject::index called on non-array object")
}

unsafe fn panic_value(_obj: *const ()) -> f64 {
    panic!("StatisticObject::value_as_f64 called on non-value object")
}
