//! Rust port of the utility types required from
//! original_clasp/clasp/util/misc_types.h.

use crate::potassco::bits;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::{Condvar, Mutex, OnceLock, PoisonError};

fn recover_lock<T>(result: Result<T, PoisonError<T>>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => error.into_inner(),
    }
}

#[must_use]
pub const fn choose(n: u32, k: u32) -> u64 {
    if k > n {
        return 0;
    }
    let mut k = k;
    let sym = n - k;
    if k > sym {
        k = sym;
    }
    let mut result = if k > 0 { n as u64 } else { 1 };
    let mut i = 2;
    while i <= k {
        result *= (n + 1 - i) as u64;
        result /= i as u64;
        i += 1;
    }
    result
}

#[must_use]
pub const fn ratio(x: u64, y: u64) -> f64 {
    if y == 0 { 0.0 } else { (x as f64) / (y as f64) }
}

#[must_use]
pub const fn ratio_with_default(x: u64, y: u64, default: f64) -> f64 {
    if y == 0 { default } else { ratio(x, y) }
}

#[must_use]
pub const fn percent(x: u64, y: u64) -> f64 {
    ratio(x, y) * 100.0
}

#[must_use]
pub fn clamp<T: Ord>(val: T, lo: T, hi: T) -> T {
    val.clamp(lo, hi)
}

pub trait SaturatingCastSource: Copy {
    const SIGNED: bool;

    fn to_i128(self) -> i128;
    fn to_u128(self) -> u128;
}

pub trait SaturatingCastTarget: Sized {
    const SIGNED: bool;
    const MIN_I128: i128;
    const MAX_I128: i128;
    const MAX_U128: u128;

    fn from_i128(value: i128) -> Self;
    fn from_u128(value: u128) -> Self;
}

macro_rules! impl_saturating_cast_signed {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl SaturatingCastSource for $ty {
                const SIGNED: bool = true;

                fn to_i128(self) -> i128 {
                    self as i128
                }

                fn to_u128(self) -> u128 {
                    self as u128
                }
            }

            impl SaturatingCastTarget for $ty {
                const SIGNED: bool = true;
                const MIN_I128: i128 = <$ty>::MIN as i128;
                const MAX_I128: i128 = <$ty>::MAX as i128;
                const MAX_U128: u128 = <$ty>::MAX as u128;

                fn from_i128(value: i128) -> Self {
                    value as Self
                }

                fn from_u128(value: u128) -> Self {
                    value as Self
                }
            }
        )+
    };
}

macro_rules! impl_saturating_cast_unsigned {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl SaturatingCastSource for $ty {
                const SIGNED: bool = false;

                fn to_i128(self) -> i128 {
                    self as i128
                }

                fn to_u128(self) -> u128 {
                    self as u128
                }
            }

            impl SaturatingCastTarget for $ty {
                const SIGNED: bool = false;
                const MIN_I128: i128 = 0;
                const MAX_I128: i128 = <$ty>::MAX as i128;
                const MAX_U128: u128 = <$ty>::MAX as u128;

                fn from_i128(value: i128) -> Self {
                    value as Self
                }

                fn from_u128(value: u128) -> Self {
                    value as Self
                }
            }
        )+
    };
}

impl_saturating_cast_signed!(i8, i16, i32, i64, i128, isize);
impl_saturating_cast_unsigned!(u8, u16, u32, u64, u128, usize);

#[must_use]
pub fn saturate_cast<Res, U>(value: U) -> Res
where
    Res: SaturatingCastTarget,
    U: SaturatingCastSource,
{
    if U::SIGNED {
        let source = value.to_i128();
        if Res::SIGNED {
            if source < Res::MIN_I128 {
                Res::from_i128(Res::MIN_I128)
            } else if source > Res::MAX_I128 {
                Res::from_i128(Res::MAX_I128)
            } else {
                Res::from_i128(source)
            }
        } else if source <= 0 {
            Res::from_u128(0)
        } else {
            let source = source as u128;
            if source > Res::MAX_U128 {
                Res::from_u128(Res::MAX_U128)
            } else {
                Res::from_u128(source)
            }
        }
    } else {
        let source = value.to_u128();
        if Res::SIGNED {
            if source > Res::MAX_I128 as u128 {
                Res::from_i128(Res::MAX_I128)
            } else {
                Res::from_i128(source as i128)
            }
        } else if source > Res::MAX_U128 {
            Res::from_u128(Res::MAX_U128)
        } else {
            Res::from_u128(source)
        }
    }
}

#[must_use]
pub fn irange(end: u32) -> Range<u32> {
    0..end
}

#[must_use]
pub fn irange_from(begin: u32, end: u32) -> Range<u32> {
    begin..end
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rng {
    seed: u32,
}

impl Default for Rng {
    fn default() -> Self {
        Self::new(1)
    }
}

impl Rng {
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self { seed }
    }

    pub const fn srand(&mut self, seed: u32) {
        self.seed = seed;
    }

    #[must_use]
    pub const fn rand(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.seed >> 16) & 0x7fff
    }

    #[must_use]
    pub fn drand(&mut self) -> f64 {
        self.rand() as f64 / 0x8000u32 as f64
    }

    #[must_use]
    pub fn irand(&mut self, max: u32) -> u32 {
        (self.drand() * max as f64) as u32
    }

    #[must_use]
    pub const fn seed(&self) -> u32 {
        self.seed
    }

    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        for index in 1..slice.len() {
            let swap_with = self.irand((index + 1) as u32) as usize;
            if index != swap_with {
                slice.swap(index, swap_with);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MovingAvgType {
    AvgSma = 0,
    AvgEma = 1,
    AvgEmaLog = 2,
    AvgEmaSmooth = 3,
    AvgEmaLogSmooth = 4,
}

#[derive(Clone, Debug)]
enum MovingAvgExtra {
    Sma(Vec<u32>),
    Alpha(f64),
    Count(u64),
}

#[derive(Clone, Debug)]
pub struct MovingAvg {
    avg: f64,
    extra: MovingAvgExtra,
    pos: u32,
    win: u32,
    full: bool,
    ema: bool,
    smooth: bool,
}

impl MovingAvg {
    #[must_use]
    pub fn new(window: u32, avg_type: MovingAvgType) -> Self {
        assert!(window > 0 || avg_type == MovingAvgType::AvgSma);
        let ema = avg_type != MovingAvgType::AvgSma;
        let full = window == 0;
        let smooth = ema && (avg_type as u32) >= (MovingAvgType::AvgEmaSmooth as u32);
        let extra = if ema {
            let alpha = if (avg_type as u32) & 1 != 0 {
                2.0 / (window + 1) as f64
            } else {
                1.0 / (1u32 << bits::log2(window)) as f64
            };
            MovingAvgExtra::Alpha(alpha)
        } else if window > 0 {
            MovingAvgExtra::Sma(vec![0; window as usize])
        } else {
            MovingAvgExtra::Count(0)
        };
        Self {
            avg: 0.0,
            extra,
            pos: 0,
            win: window,
            full,
            ema,
            smooth,
        }
    }

    #[must_use]
    pub const fn ema(current: f64, sample: f64, alpha: f64) -> f64 {
        current + (alpha * (sample - current))
    }

    #[must_use]
    pub const fn cma(current: f64, sample: f64, num_seen: u64) -> f64 {
        (sample + (current * num_seen as f64)) / (num_seen + 1) as f64
    }

    #[must_use]
    pub fn sma(
        current: f64,
        sample: u32,
        buffer: &mut [u32],
        cap: u32,
        pos: u32,
        full: bool,
    ) -> f64 {
        assert!(pos < cap);
        let old_sample = buffer[pos as usize] as f64;
        let new_sample = sample as f64;
        buffer[pos as usize] = sample;
        if full {
            current + ((new_sample - old_sample) / cap as f64)
        } else {
            Self::cma(current, new_sample, pos as u64)
        }
    }

    #[must_use]
    pub fn smooth_alpha(alpha: f64, pos: u32) -> f64 {
        if pos < 32 {
            alpha.max(1.0 / (1u32 << pos) as f64)
        } else {
            alpha
        }
    }

    pub fn push(&mut self, value: u32) -> bool {
        let is_valid = self.valid();
        let pos = self.pos;
        let win = self.win;
        let smooth = self.smooth;
        match &mut self.extra {
            MovingAvgExtra::Count(num_seen) if win == 0 => {
                self.avg = Self::cma(self.avg, value as f64, *num_seen);
                *num_seen += 1;
            }
            MovingAvgExtra::Sma(buffer) if !self.ema => {
                self.avg = Self::sma(self.avg, value, buffer, win, pos, is_valid);
            }
            MovingAvgExtra::Alpha(alpha) if is_valid => {
                self.avg = Self::ema(self.avg, value as f64, *alpha);
            }
            MovingAvgExtra::Alpha(alpha) => {
                self.avg = if smooth {
                    Self::ema(self.avg, value as f64, Self::smooth_alpha(*alpha, pos))
                } else {
                    Self::cma(self.avg, value as f64, pos as u64)
                };
            }
            _ => unreachable!(),
        }

        self.pos = self.pos.wrapping_add(1);
        if self.pos == self.win {
            self.pos = 0;
            self.full = true;
        }
        self.valid()
    }

    pub fn clear(&mut self) {
        self.avg = 0.0;
        self.pos = 0;
        if self.win == 0 {
            self.extra = MovingAvgExtra::Count(0);
        } else {
            self.full = false;
        }
    }

    #[must_use]
    pub const fn get(&self) -> f64 {
        self.avg
    }

    #[must_use]
    pub const fn valid(&self) -> bool {
        self.full
    }

    #[must_use]
    pub const fn win(&self) -> u32 {
        self.win
    }
}

pub trait Destroy {
    fn destroy(&mut self);
}

pub trait Release {
    fn release(&mut self);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DestroyObject;

impl DestroyObject {
    pub fn apply<T: Destroy>(&self, object: Option<&mut T>) {
        if let Some(object) = object {
            object.destroy();
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeleteObject;

impl DeleteObject {
    pub fn apply<T>(&self, object: T) {
        drop(object);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ReleaseObject;

impl ReleaseObject {
    pub fn apply<T: Release>(&self, object: Option<&mut T>) {
        if let Some(object) = object {
            object.release();
        }
    }
}

#[derive(Debug)]
pub struct TaggedPtr<T, const N: usize = 1> {
    ptr: usize,
    marker: PhantomData<*mut T>,
}

impl<T, const N: usize> Clone for TaggedPtr<T, N> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, const N: usize> Copy for TaggedPtr<T, N> {}

impl<T, const N: usize> Default for TaggedPtr<T, N> {
    fn default() -> Self {
        Self {
            ptr: 0,
            marker: PhantomData,
        }
    }
}

impl<T, const N: usize> TaggedPtr<T, N> {
    fn mask() -> usize {
        assert!(N > 0);
        assert!(N < usize::BITS as usize);
        let mask = bits::bit_max::<usize>(N as u32);
        assert!(std::mem::align_of::<T>() > mask);
        mask
    }

    #[must_use]
    pub fn new(ptr: *mut T) -> Self {
        Self {
            ptr: ptr as usize,
            marker: PhantomData,
        }
    }

    #[must_use]
    pub fn with_all_tags(ptr: *mut T) -> Self {
        Self {
            ptr: (ptr as usize) | Self::mask(),
            marker: PhantomData,
        }
    }

    #[must_use]
    pub fn test<const I: usize>(&self) -> bool {
        assert!(I < N);
        bits::test_bit(self.ptr, I as u32)
    }

    #[must_use]
    pub fn any(&self) -> bool {
        bits::test_any(self.ptr, Self::mask())
    }

    pub fn set<const I: usize>(&mut self) {
        assert!(I < N);
        bits::store_set_bit(&mut self.ptr, I as u32);
    }

    pub fn clear<const I: usize>(&mut self) {
        assert!(I < N);
        bits::store_clear_bit(&mut self.ptr, I as u32);
    }

    pub fn toggle<const I: usize>(&mut self) {
        assert!(I < N);
        bits::store_toggle_bit(&mut self.ptr, I as u32);
    }

    #[must_use]
    pub fn get(self) -> *mut T {
        bits::clear_mask(self.ptr, Self::mask()) as *mut T
    }

    #[must_use]
    pub fn is_null(self) -> bool {
        self.get().is_null()
    }

    pub fn swap(&mut self, other: &mut Self) {
        std::mem::swap(&mut self.ptr, &mut other.ptr);
    }
}

impl<T, const N: usize> From<*mut T> for TaggedPtr<T, N> {
    fn from(value: *mut T) -> Self {
        Self::new(value)
    }
}

impl<T, const N: usize> From<TaggedPtr<T, N>> for bool {
    fn from(value: TaggedPtr<T, N>) -> Self {
        !value.is_null()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range32 {
    pub lo: u32,
    pub hi: u32,
}

impl Range32 {
    #[must_use]
    pub const fn new(x: u32, y: u32) -> Self {
        if x <= y {
            Self { lo: x, hi: y }
        } else {
            Self { lo: y, hi: x }
        }
    }

    #[must_use]
    pub fn clamp(&self, value: u32) -> u32 {
        value.clamp(self.lo, self.hi)
    }
}

impl From<(u32, u32)> for Range32 {
    fn from(value: (u32, u32)) -> Self {
        Self::new(value.0, value.1)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Subsystem {
    SubsystemFacade = 0,
    SubsystemLoad = 1,
    SubsystemPrepare = 2,
    SubsystemSolve = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Verbosity {
    VerbosityQuiet = 0,
    VerbosityLow = 1,
    VerbosityHigh = 2,
    VerbosityMax = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Event {
    pub system: u32,
    pub verb: u32,
    pub op: u32,
    pub id: u32,
}

impl Event {
    #[must_use]
    pub fn for_type<T: 'static>(system: Subsystem, verbosity: Verbosity) -> Self {
        Self {
            system: system as u32,
            verb: verbosity as u32,
            op: 0,
            id: event_id::<T>(),
        }
    }

    #[must_use]
    pub fn next_id() -> u32 {
        static NEXT_ID: AtomicU32 = AtomicU32::new(0);
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    }
}

fn event_ids() -> &'static Mutex<HashMap<TypeId, u32>> {
    static IDS: OnceLock<Mutex<HashMap<TypeId, u32>>> = OnceLock::new();
    IDS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[must_use]
pub fn event_id<T: 'static>() -> u32 {
    let type_id = TypeId::of::<T>();
    let mut ids = recover_lock(event_ids().lock());
    if let Some(id) = ids.get(&type_id) {
        *id
    } else {
        let id = Event::next_id();
        ids.insert(type_id, id);
        id
    }
}

pub trait EventLike: Any {
    fn event(&self) -> &Event;

    fn as_any(&self) -> &dyn Any;
}

#[must_use]
pub fn event_cast<T: EventLike + 'static>(event: &dyn EventLike) -> Option<&T> {
    if event.event().id == event_id::<T>() {
        event.as_any().downcast_ref::<T>()
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnterEvent {
    pub base: Event,
}

impl EnterEvent {
    #[must_use]
    pub fn new(system: Subsystem, verbosity: Verbosity) -> Self {
        Self {
            base: Event::for_type::<Self>(system, verbosity),
        }
    }
}

impl EventLike for EnterEvent {
    fn event(&self) -> &Event {
        &self.base
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug)]
pub struct Overload<T>(pub T);

#[derive(Debug)]
struct LockedValueState<T> {
    value: T,
    locked: bool,
}

#[derive(Debug)]
pub struct LockedValue<T = u32> {
    state: Mutex<LockedValueState<T>>,
    ready: Condvar,
}

unsafe impl<T: Copy> Send for LockedValue<T> {}
unsafe impl<T: Copy> Sync for LockedValue<T> {}

impl<T: Copy + Default> Default for LockedValue<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Copy> LockedValue<T> {
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            state: Mutex::new(LockedValueState {
                value,
                locked: false,
            }),
            ready: Condvar::new(),
        }
    }

    #[must_use]
    pub fn value(&self) -> T {
        recover_lock(self.state.lock()).value
    }

    #[must_use]
    pub fn try_lock(&self) -> bool {
        let mut state = recover_lock(self.state.lock());
        if state.locked {
            false
        } else {
            state.locked = true;
            true
        }
    }

    #[must_use]
    pub fn try_lock_with_value(&self, out: &mut T) -> bool {
        let mut state = recover_lock(self.state.lock());
        *out = state.value;
        if state.locked {
            false
        } else {
            state.locked = true;
            true
        }
    }

    #[must_use]
    pub fn lock(&self) -> T {
        let mut state = recover_lock(self.state.lock());
        loop {
            if !state.locked {
                state.locked = true;
                return state.value;
            }
            state = recover_lock(self.ready.wait(state));
        }
    }

    pub fn store_unlock(&self, value: T) {
        let mut state = recover_lock(self.state.lock());
        state.value = value;
        state.locked = false;
        self.ready.notify_one();
    }
}

impl<T: Copy + Default> LockedValue<T> {
    pub fn unlock(&self) {
        self.store_unlock(T::default());
    }
}

#[derive(Debug)]
pub struct RefCount {
    rc: AtomicU32,
}

impl RefCount {
    #[must_use]
    pub fn new(init: u32) -> Self {
        Self {
            rc: AtomicU32::new(init),
        }
    }

    #[must_use]
    pub fn count(&self) -> u32 {
        self.rc.load(Ordering::Acquire)
    }

    pub fn reset(&self, value: u32) {
        self.rc.store(value, Ordering::Relaxed);
    }

    pub fn add(&self, delta: u32) {
        self.rc.fetch_add(delta, Ordering::Relaxed);
    }

    #[must_use]
    pub fn release(&self, delta: u32) -> bool {
        self.release_fetch(delta) == 0
    }

    #[must_use]
    pub fn release_fetch(&self, delta: u32) -> u32 {
        self.rc.fetch_sub(delta, Ordering::AcqRel) - delta
    }
}

impl Default for RefCount {
    fn default() -> Self {
        Self::new(1)
    }
}

impl From<u32> for RefCount {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl From<&RefCount> for u32 {
    fn from(value: &RefCount) -> Self {
        value.count()
    }
}

#[derive(Debug, Default)]
pub struct SigAtomic {
    sig: AtomicI32,
}

impl SigAtomic {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn value(&self) -> i32 {
        self.sig.load(Ordering::Acquire)
    }

    pub fn set_if_unset(&self, sig: i32) -> bool {
        self.sig
            .compare_exchange(0, sig, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    #[must_use]
    pub fn exchange(&self, sig: i32) -> i32 {
        self.sig.swap(sig, Ordering::Acquire)
    }
}
