//! Rust port of original_clasp/libpotassco/potassco/utils.h.

use core::cmp::Ordering;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::mem;
use core::ptr::NonNull;
use core::slice;
use std::collections::HashMap;

use crate::potassco::bits::{bit_count, store_clear_bit, store_set_bit, test_bit};

const FAST_GROW_CAP: usize = 0x20000;

fn next_capacity(current: usize) -> usize {
    if current == 0 {
        64
    } else if current <= FAST_GROW_CAP {
        (current * 3 + 1) >> 1
    } else {
        current << 1
    }
}

enum BufferStorage {
    Owned(Vec<u8>),
    Borrowed { ptr: NonNull<u8>, cap: usize },
}

pub struct DynamicBuffer {
    storage: BufferStorage,
    len: usize,
}

impl Default for DynamicBuffer {
    fn default() -> Self {
        Self {
            storage: BufferStorage::Owned(Vec::new()),
            len: 0,
        }
    }
}

impl Clone for DynamicBuffer {
    fn clone(&self) -> Self {
        let mut out = Self::default();
        out.append_bytes(self.as_bytes());
        out
    }
}

impl DynamicBuffer {
    pub fn new(initial_cap: usize) -> Self {
        let mut buffer = Self::default();
        buffer.reserve(initial_cap);
        buffer
    }

    pub fn borrowed(buffer: &mut [u8]) -> Self {
        let ptr = NonNull::new(buffer.as_mut_ptr()).unwrap_or_else(NonNull::dangling);
        Self {
            storage: BufferStorage::Borrowed {
                ptr,
                cap: buffer.len(),
            },
            len: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        match &self.storage {
            BufferStorage::Owned(vec) => vec.capacity(),
            BufferStorage::Borrowed { cap, .. } => *cap,
        }
    }

    pub fn size(&self) -> usize {
        self.len
    }

    pub fn as_ptr(&self) -> *const u8 {
        match &self.storage {
            BufferStorage::Owned(vec) => vec.as_ptr(),
            BufferStorage::Borrowed { ptr, .. } => ptr.as_ptr(),
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        match &mut self.storage {
            BufferStorage::Owned(vec) => vec.as_mut_ptr(),
            BufferStorage::Borrowed { ptr, .. } => ptr.as_ptr(),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match &self.storage {
            BufferStorage::Owned(vec) => &vec[..self.len],
            BufferStorage::Borrowed { ptr, .. } => {
                // SAFETY: borrowed buffers are created from valid mutable slices and `len` never exceeds `cap`.
                unsafe { slice::from_raw_parts(ptr.as_ptr(), self.len) }
            }
        }
    }

    pub fn view(&self) -> &str {
        // SAFETY: this utility buffer mirrors the upstream string-oriented API and is used with ASCII/UTF-8 data.
        unsafe { core::str::from_utf8_unchecked(self.as_bytes()) }
    }

    pub fn reserve(&mut self, n: usize) {
        if n <= self.capacity() {
            return;
        }

        let new_cap = next_capacity(self.capacity()).max(n);
        let current = self.as_bytes().to_vec();
        let mut owned = Vec::with_capacity(new_cap);
        owned.extend_from_slice(&current);
        self.storage = BufferStorage::Owned(owned);
    }

    pub fn alloc(&mut self, n: usize) -> &mut [u8] {
        let start = self.len;
        self.reserve(self.len + n);
        self.len += n;
        match &mut self.storage {
            BufferStorage::Owned(vec) => {
                if vec.len() < self.len {
                    vec.resize(self.len, 0);
                }
                &mut vec[start..self.len]
            }
            BufferStorage::Borrowed { ptr, .. } => {
                // SAFETY: capacity has been checked and `start..len` lies within the borrowed allocation.
                unsafe { slice::from_raw_parts_mut(ptr.as_ptr().add(start), n) }
            }
        }
    }

    pub fn append_bytes(&mut self, data: &[u8]) -> &mut Self {
        if !data.is_empty() {
            self.alloc(data.len()).copy_from_slice(data);
        }
        self
    }

    pub fn append_str(&mut self, data: &str) -> &mut Self {
        self.append_bytes(data.as_bytes())
    }

    pub fn push(&mut self, byte: u8) {
        self.alloc(1)[0] = byte;
    }

    pub fn back(&mut self) -> &mut u8 {
        assert!(self.len > 0);
        match &mut self.storage {
            BufferStorage::Owned(vec) => &mut vec[self.len - 1],
            BufferStorage::Borrowed { ptr, .. } => {
                // SAFETY: len > 0 and the borrowed storage is valid for `len` bytes.
                unsafe { &mut *ptr.as_ptr().add(self.len - 1) }
            }
        }
    }

    pub fn pop(&mut self, n: usize) {
        self.len -= n.min(self.len);
        if let BufferStorage::Owned(vec) = &mut self.storage {
            vec.truncate(self.len);
        }
    }

    pub fn clear(&mut self) {
        self.len = 0;
        if let BufferStorage::Owned(vec) = &mut self.storage {
            vec.clear();
        }
    }

    pub fn release(&mut self) {
        self.storage = BufferStorage::Owned(Vec::new());
        self.len = 0;
    }

    pub fn swap(&mut self, other: &mut Self) {
        mem::swap(self, other);
    }
}

#[derive(Debug, Default)]
pub struct DynamicBitset {
    words: Vec<u64>,
}

impl DynamicBitset {
    pub fn reserve(&mut self, num_bits: u32) {
        if num_bits != 0 {
            let words = (1 + (num_bits / 64)) as usize;
            self.words.reserve(words.saturating_sub(self.words.len()));
        }
    }

    pub fn contains(&self, bit: u32) -> bool {
        let (word, pos) = Self::idx(bit);
        self.words.get(word).is_some_and(|w| test_bit(*w, pos))
    }

    pub fn empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn count(&self) -> u32 {
        self.words
            .iter()
            .fold(0, |count, word| count + bit_count(*word))
    }

    pub fn smallest(&self) -> u32 {
        self.words
            .first()
            .map(|word| word.trailing_zeros())
            .unwrap_or(0)
    }

    pub fn largest(&self) -> u32 {
        match self.words.last() {
            None => 0,
            Some(word) => ((self.words.len() as u32 - 1) * 64) + (63 - word.leading_zeros()),
        }
    }

    pub fn words(&self) -> u32 {
        self.words.len() as u32
    }

    pub fn add(&mut self, bit: u32) -> bool {
        let (word, pos) = Self::idx(bit);
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        if test_bit(self.words[word], pos) {
            false
        } else {
            store_set_bit(&mut self.words[word], pos);
            true
        }
    }

    pub fn remove(&mut self, bit: u32) -> bool {
        let (word, pos) = Self::idx(bit);
        if word < self.words.len() && test_bit(self.words[word], pos) {
            if store_clear_bit(&mut self.words[word], pos) == 0 {
                self.compact();
            }
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.words.clear();
    }

    pub fn apply(&mut self, mask: u64) {
        for word in &mut self.words {
            *word &= mask;
        }
        self.compact();
    }

    fn idx(bit: u32) -> (usize, u32) {
        ((bit / 64) as usize, bit & 63)
    }

    fn compact(&mut self) {
        while self.words.last().copied() == Some(0) {
            self.words.pop();
        }
    }
}

impl PartialEq for DynamicBitset {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for DynamicBitset {}

impl PartialOrd for DynamicBitset {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DynamicBitset {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.words.len().cmp(&other.words.len()) {
            Ordering::Equal => {
                for (lhs, rhs) in self.words.iter().zip(other.words.iter()).rev() {
                    match lhs.cmp(rhs) {
                        Ordering::Equal => {}
                        ordering => return ordering,
                    }
                }
                Ordering::Equal
            }
            ordering => ordering,
        }
    }
}

enum ConstStringRepr {
    Small { len: u8, data: [u8; 24] },
    Owned { data: Box<[u8]>, len: usize },
    Borrowed { ptr: NonNull<u8>, len: usize },
}

pub struct ConstString {
    repr: ConstStringRepr,
}

impl Default for ConstString {
    fn default() -> Self {
        Self {
            repr: ConstStringRepr::Small {
                len: 0,
                data: [0; 24],
            },
        }
    }
}

impl Clone for ConstString {
    fn clone(&self) -> Self {
        Self::from(self.view())
    }
}

impl From<&str> for ConstString {
    fn from(value: &str) -> Self {
        if value.len() <= 23 {
            let mut data = [0u8; 24];
            data[..value.len()].copy_from_slice(value.as_bytes());
            Self {
                repr: ConstStringRepr::Small {
                    len: value.len() as u8,
                    data,
                },
            }
        } else {
            let mut bytes = Vec::with_capacity(value.len() + 1);
            bytes.extend_from_slice(value.as_bytes());
            bytes.push(0);
            Self {
                repr: ConstStringRepr::Owned {
                    data: bytes.into_boxed_slice(),
                    len: value.len(),
                },
            }
        }
    }
}

impl From<String> for ConstString {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl ConstString {
    /// # Safety
    ///
    /// The caller must ensure that `value` outlives the returned `ConstString`
    /// and remains valid UTF-8 for the entire time the borrowed representation
    /// is used.
    pub unsafe fn borrowed(value: &str) -> Self {
        let ptr = NonNull::new(value.as_ptr() as *mut u8).unwrap_or_else(NonNull::dangling);
        Self {
            repr: ConstStringRepr::Borrowed {
                ptr,
                len: value.len(),
            },
        }
    }

    pub fn c_str(&self) -> *const u8 {
        match &self.repr {
            ConstStringRepr::Small { data, .. } => data.as_ptr(),
            ConstStringRepr::Owned { data, .. } => data.as_ptr(),
            ConstStringRepr::Borrowed { ptr, .. } => ptr.as_ptr(),
        }
    }

    pub fn view(&self) -> &str {
        let bytes = match &self.repr {
            ConstStringRepr::Small { len, data } => &data[..*len as usize],
            ConstStringRepr::Owned { data, len } => &data[..*len],
            ConstStringRepr::Borrowed { ptr, len } => {
                // SAFETY: caller of `borrowed()` guarantees the pointee remains valid.
                unsafe { slice::from_raw_parts(ptr.as_ptr(), *len) }
            }
        };
        // SAFETY: upstream string utilities operate on valid string data.
        unsafe { core::str::from_utf8_unchecked(bytes) }
    }

    pub fn size(&self) -> usize {
        self.view().len()
    }

    pub fn small(&self) -> bool {
        matches!(self.repr, ConstStringRepr::Small { .. })
    }
}

impl PartialEq for ConstString {
    fn eq(&self, other: &Self) -> bool {
        self.view() == other.view()
    }
}

impl Eq for ConstString {}

impl PartialOrd for ConstString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ConstString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.view().cmp(other.view())
    }
}

impl Hash for ConstString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.view().hash(state);
    }
}

pub type StringMap<V> = HashMap<ConstString, V>;

pub fn try_emplace<V>(map: &mut StringMap<V>, key: &str, value: V) -> bool {
    if map.contains_key(key) {
        false
    } else {
        map.insert(ConstString::from(key), value);
        true
    }
}

pub struct Temp<T> {
    buffer: Vec<T>,
}

impl<T> Default for Temp<T> {
    fn default() -> Self {
        Self { buffer: Vec::new() }
    }
}

impl<T: Clone> Temp<T> {
    pub fn resize(&mut self, n: usize, value: T) {
        self.buffer.resize(n, value);
    }

    pub fn data(&mut self) -> *mut T {
        self.buffer.as_mut_ptr()
    }

    pub fn as_slice(&self) -> &[T] {
        &self.buffer
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.buffer
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RadixConfig {
    pub std_sort_threshold: usize,
    pub std_stable: bool,
}

impl RadixConfig {
    pub const DEF_THRESHOLD: usize = 32;
}

pub const RADIX_DEF: RadixConfig = RadixConfig {
    std_sort_threshold: 0,
    std_stable: true,
};

pub const RADIX_RELAXED: RadixConfig = RadixConfig {
    std_sort_threshold: 0,
    std_stable: false,
};

pub const RADIX_ONLY: RadixConfig = RadixConfig {
    std_sort_threshold: 1,
    std_stable: true,
};

pub trait RadixRank: Copy + Ord {
    const BYTES: usize;
    fn byte_at(self, pass: usize) -> u8;
}

macro_rules! impl_radix_rank {
	($($ty:ty),+ $(,)?) => {
		$(
			impl RadixRank for $ty {
				const BYTES: usize = mem::size_of::<$ty>();

				fn byte_at(self, pass: usize) -> u8 {
					((self >> (pass * 8)) & 0xFF) as u8
				}
			}
		)+
	};
}

impl_radix_rank!(u8, u16, u32, u64, u128, usize);

pub fn enumerate<I>(iterable: I) -> impl Iterator<Item = (usize, I::Item)>
where
    I: IntoIterator,
{
    iterable.into_iter().enumerate()
}

pub fn radix_sort<T, R, F>(slice: &mut [T], rank: F, config: RadixConfig)
where
    T: Clone,
    R: RadixRank,
    F: Fn(&T) -> R,
{
    let mut temp = Vec::new();
    radix_sort_with_buffer(slice, rank, config, &mut temp);
}

pub fn radix_sort_with_buffer<T, R, F>(
    slice: &mut [T],
    rank: F,
    config: RadixConfig,
    temp: &mut Vec<T>,
) where
    T: Clone,
    R: RadixRank,
    F: Fn(&T) -> R,
{
    if slice.len() < 2 {
        return;
    }

    let threshold = if config.std_sort_threshold == 0 {
        RadixConfig::DEF_THRESHOLD
    } else {
        config.std_sort_threshold
    };
    if slice.len() < threshold {
        if config.std_stable {
            slice.sort_by_key(|value| rank(value));
        } else {
            slice.sort_unstable_by_key(|value| rank(value));
        }
        return;
    }

    let mut from = slice.to_vec();
    temp.clear();
    if let Some(first) = slice.first() {
        temp.resize(slice.len(), first.clone());
    }

    for pass in 0..R::BYTES {
        let mut count = [0usize; 256];
        let mut sorted = true;
        let mut prev = 0u8;
        for (index, item) in from.iter().enumerate() {
            let digit = rank(item).byte_at(pass);
            count[digit as usize] += 1;
            if index != 0 && digit < prev {
                sorted = false;
            }
            prev = digit;
        }
        if sorted {
            continue;
        }
        let mut total = 0usize;
        for slot in &mut count {
            let next = total + *slot;
            *slot = total;
            total = next;
        }
        for item in &from {
            let digit = rank(item).byte_at(pass) as usize;
            temp[count[digit]] = item.clone();
            count[digit] += 1;
        }
        mem::swap(&mut from, temp);
    }

    slice.clone_from_slice(&from);
}

impl std::borrow::Borrow<str> for ConstString {
    fn borrow(&self) -> &str {
        self.view()
    }
}

pub struct EnumerateTag<T>(PhantomData<T>);
