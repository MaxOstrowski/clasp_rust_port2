//! Rust port of `original_clasp/clasp/util/pod_vector.h`.

use std::cmp::Ordering;
use std::fmt;
use std::iter::FromIterator;
use std::mem::{MaybeUninit, size_of};
use std::ops::{Bound, Deref, DerefMut, Index, IndexMut, RangeBounds};
use std::ptr;
use std::slice;

mod detail {
    use super::MaybeUninit;
    use std::ptr;

    pub(crate) unsafe fn fill<T: std::marker::Copy>(
        dst: *mut MaybeUninit<T>,
        count: usize,
        value: T,
    ) {
        for index in 0..count {
            unsafe {
                dst.add(index).write(MaybeUninit::new(value));
            }
        }
    }

    pub(crate) unsafe fn copy<T: std::marker::Copy>(
        src: *const T,
        count: usize,
        dst: *mut MaybeUninit<T>,
    ) {
        unsafe {
            ptr::copy_nonoverlapping(src, dst.cast::<T>(), count);
        }
    }

    pub(crate) struct Copy<I> {
        iter: I,
    }

    impl<I> Copy<I> {
        pub(crate) fn new(iter: I) -> Self {
            Self { iter }
        }
    }

    impl<I, T> Copy<I>
    where
        I: IntoIterator<Item = T>,
        T: std::marker::Copy,
    {
        pub(crate) unsafe fn write_into(self, dst: *mut MaybeUninit<T>) -> usize {
            let mut written = 0;
            for value in self.iter {
                unsafe {
                    dst.add(written).write(MaybeUninit::new(value));
                }
                written += 1;
            }
            written
        }
    }

    pub(crate) struct Memcpy<'a, T> {
        src: &'a [T],
    }

    impl<'a, T> Memcpy<'a, T> {
        pub(crate) fn new(src: &'a [T]) -> Self {
            Self { src }
        }
    }

    impl<T: std::marker::Copy> Memcpy<'_, T> {
        pub(crate) unsafe fn write_into(self, dst: *mut MaybeUninit<T>) -> usize {
            unsafe {
                copy(self.src.as_ptr(), self.src.len(), dst);
            }
            self.src.len()
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutOfRangeError;

impl fmt::Display for OutOfRangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("pod_vector::at")
    }
}

impl std::error::Error for OutOfRangeError {}

/// POD-oriented vector modeled after the upstream `bk_lib::pod_vector`.
pub struct PodVector<T: Copy> {
    storage: Vec<MaybeUninit<T>>,
    len: usize,
}

impl<T: Copy> Default for PodVector<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy> PodVector<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            storage: Vec::new(),
            len: 0,
        }
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            storage: Self::allocate(capacity),
            len: 0,
        }
    }

    #[must_use]
    pub fn with_size(size: usize, value: T) -> Self {
        let mut vector = Self::with_capacity(size);
        unsafe {
            detail::fill(vector.storage.as_mut_ptr(), size, value);
        }
        vector.len = size;
        vector
    }

    #[must_use]
    pub fn from_slice(slice: &[T]) -> Self {
        let mut vector = Self::with_capacity(slice.len());
        unsafe {
            detail::Memcpy::new(slice).write_into(vector.storage.as_mut_ptr());
        }
        vector.len = slice.len();
        vector
    }

    #[must_use]
    pub fn size(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn max_size(&self) -> usize {
        if size_of::<T>() == 0 {
            usize::MAX
        } else {
            usize::MAX / size_of::<T>()
        }
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.storage.len()
    }

    #[must_use]
    pub fn empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.empty()
    }

    #[must_use]
    pub fn data(&self) -> *const T {
        if self.capacity() == 0 {
            ptr::null()
        } else {
            self.base_ptr()
        }
    }

    #[must_use]
    pub fn begin(&self) -> *const T {
        self.data()
    }

    #[must_use]
    pub fn end(&self) -> *const T {
        if self.len == 0 {
            self.begin()
        } else {
            unsafe { self.base_ptr().add(self.len) }
        }
    }

    #[must_use]
    pub fn data_mut(&mut self) -> *mut T {
        if self.capacity() == 0 {
            ptr::null_mut()
        } else {
            self.base_ptr_mut()
        }
    }

    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.base_ptr(), self.len) }
    }

    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.base_ptr_mut(), self.len) }
    }

    pub fn at(&self, index: usize) -> Result<&T, OutOfRangeError> {
        self.as_slice().get(index).ok_or(OutOfRangeError)
    }

    pub fn at_mut(&mut self, index: usize) -> Result<&mut T, OutOfRangeError> {
        self.as_mut_slice().get_mut(index).ok_or(OutOfRangeError)
    }

    #[must_use]
    pub fn front(&self) -> &T {
        assert!(!self.empty());
        &self[0]
    }

    #[must_use]
    pub fn front_mut(&mut self) -> &mut T {
        assert!(!self.empty());
        &mut self[0]
    }

    #[must_use]
    pub fn back(&self) -> &T {
        assert!(!self.empty());
        &self[self.len - 1]
    }

    #[must_use]
    pub fn back_mut(&mut self) -> &mut T {
        assert!(!self.empty());
        let index = self.len - 1;
        &mut self[index]
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn assign_fill(&mut self, count: usize, value: T) {
        self.clear();
        self.insert_n(self.len, count, value);
    }

    pub fn assign_from_slice(&mut self, values: &[T]) {
        self.clear();
        self.insert_slice(self.len, values);
    }

    pub fn assign<I>(&mut self, values: I)
    where
        I: IntoIterator<Item = T>,
    {
        self.clear();
        self.extend(values);
    }

    pub fn erase(&mut self, index: usize) -> usize {
        assert!(!self.empty());
        assert!(index < self.len);
        self.erase_range(index..index + 1)
    }

    pub fn erase_range<R>(&mut self, range: R) -> usize
    where
        R: RangeBounds<usize>,
    {
        let (start, end) = self.normalize_range(range);
        let removed = end.saturating_sub(start);
        if removed == 0 {
            return start;
        }
        unsafe {
            ptr::copy(
                self.base_ptr().add(end),
                self.base_ptr_mut().add(start),
                self.len - end,
            );
        }
        self.len -= removed;
        start
    }

    pub fn resize(&mut self, new_size: usize, value: T) {
        if new_size > self.len {
            let additional = new_size - self.len;
            if new_size > self.capacity() {
                self.append_realloc(additional, value);
                return;
            }
            unsafe {
                detail::fill(self.storage.as_mut_ptr().add(self.len), additional, value);
            }
        }
        self.len = new_size;
    }

    pub fn reserve(&mut self, capacity: usize) {
        if capacity <= self.capacity() {
            return;
        }
        let mut next = Self::allocate(capacity);
        unsafe {
            ptr::copy_nonoverlapping(self.storage.as_ptr(), next.as_mut_ptr(), self.len);
        }
        self.storage = next;
    }

    pub fn swap(&mut self, other: &mut Self) {
        std::mem::swap(self, other);
    }

    pub fn push_back(&mut self, value: T) {
        if self.len == self.capacity() {
            self.append_realloc(1, value);
            return;
        }
        unsafe {
            self.storage
                .as_mut_ptr()
                .add(self.len)
                .write(MaybeUninit::new(value));
        }
        self.len += 1;
    }

    pub fn pop_back(&mut self) {
        assert!(!self.empty());
        self.len -= 1;
    }

    pub fn insert(&mut self, index: usize, value: T) -> usize {
        self.insert_n(index, 1, value);
        index
    }

    pub fn insert_n(&mut self, index: usize, count: usize, value: T) -> usize {
        assert!(index <= self.len);
        if count == 0 {
            return index;
        }
        self.ensure_insert_capacity(count);
        self.move_right(index, count);
        unsafe {
            detail::fill(self.storage.as_mut_ptr().add(index), count, value);
        }
        self.len += count;
        index
    }

    pub fn insert_slice(&mut self, index: usize, values: &[T]) -> usize {
        assert!(index <= self.len);
        if values.is_empty() {
            return index;
        }
        self.ensure_insert_capacity(values.len());
        self.move_right(index, values.len());
        unsafe {
            detail::Memcpy::new(values).write_into(self.storage.as_mut_ptr().add(index));
        }
        self.len += values.len();
        index
    }

    pub fn insert_range<I>(&mut self, index: usize, values: I) -> usize
    where
        I: IntoIterator<Item = T>,
    {
        assert!(index <= self.len);
        let collected: Vec<T> = values.into_iter().collect();
        self.insert_slice(index, &collected)
    }

    /// Extends the logical size without initializing the new elements.
    ///
    /// # Safety
    ///
    /// The caller must initialize every element in the newly exposed range before any read,
    /// iteration, comparison, or safe slice access touches those elements.
    pub unsafe fn resize_no_init(&mut self, new_size: usize) {
        if new_size > self.capacity() {
            self.reserve(new_size);
        }
        self.len = new_size;
    }

    fn allocate(capacity: usize) -> Vec<MaybeUninit<T>> {
        let mut storage = Vec::with_capacity(capacity);
        unsafe {
            storage.set_len(capacity);
        }
        storage
    }

    fn base_ptr(&self) -> *const T {
        self.storage.as_ptr().cast::<T>()
    }

    fn base_ptr_mut(&mut self) -> *mut T {
        self.storage.as_mut_ptr().cast::<T>()
    }

    fn grow_size(&self, additional: usize) -> usize {
        let mut new_capacity = self.len + additional;
        assert!(new_capacity > self.len, "pod_vector: max size exceeded!");
        assert!(new_capacity > self.capacity());
        if new_capacity < 4 {
            new_capacity = 1usize << (new_capacity + 1);
        }
        let scaled = (self.capacity() * 3) >> 1;
        if new_capacity < scaled {
            new_capacity = scaled;
        }
        new_capacity
    }

    fn append_realloc(&mut self, count: usize, value: T) {
        let new_capacity = self.grow_size(count);
        let mut next = Self::allocate(new_capacity);
        unsafe {
            ptr::copy_nonoverlapping(self.storage.as_ptr(), next.as_mut_ptr(), self.len);
            detail::fill(next.as_mut_ptr().add(self.len), count, value);
        }
        self.storage = next;
        self.len += count;
    }

    fn ensure_insert_capacity(&mut self, count: usize) {
        assert!(
            count == 0 || self.len + count > self.len,
            "pod_vector: max size exceeded!"
        );
        if self.len + count > self.capacity() {
            self.reserve(self.grow_size(count));
        }
    }

    fn move_right(&mut self, index: usize, count: usize) {
        assert!(index <= self.len);
        assert!(self.capacity() - index >= count);
        if index < self.len {
            unsafe {
                ptr::copy(
                    self.base_ptr().add(index),
                    self.base_ptr_mut().add(index + count),
                    self.len - index,
                );
            }
        }
    }

    fn normalize_range<R>(&self, range: R) -> (usize, usize)
    where
        R: RangeBounds<usize>,
    {
        let start = match range.start_bound() {
            Bound::Included(&value) => value,
            Bound::Excluded(&value) => value + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&value) => value + 1,
            Bound::Excluded(&value) => value,
            Bound::Unbounded => self.len,
        };
        assert!(start <= end);
        assert!(end <= self.len);
        (start, end)
    }
}

impl<T: Copy> Clone for PodVector<T> {
    fn clone(&self) -> Self {
        Self::from_slice(self.as_slice())
    }
}

impl<T: Copy> AsRef<[T]> for PodVector<T> {
    fn as_ref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T: Copy> AsMut<[T]> for PodVector<T> {
    fn as_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl<T: Copy> Deref for PodVector<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T: Copy> DerefMut for PodVector<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<T: Copy> Extend<T> for PodVector<T> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        let values: Vec<T> = iter.into_iter().collect();
        let start = self.len;
        self.ensure_insert_capacity(values.len());
        unsafe {
            detail::Copy::new(values.iter().copied())
                .write_into(self.storage.as_mut_ptr().add(start));
        }
        self.len += values.len();
    }
}

impl<T: Copy> FromIterator<T> for PodVector<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let values: Vec<T> = iter.into_iter().collect();
        Self::from_slice(&values)
    }
}

impl<T: Copy + fmt::Debug> fmt::Debug for PodVector<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl<T: Copy + PartialEq> PartialEq for PodVector<T> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T: Copy + Eq> Eq for PodVector<T> {}

impl<T: Copy + PartialOrd> PartialOrd for PodVector<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.as_slice().partial_cmp(other.as_slice())
    }
}

impl<T: Copy + Ord> Ord for PodVector<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl<T: Copy> Index<usize> for PodVector<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index < self.len);
        unsafe { &*self.base_ptr().add(index) }
    }
}

impl<T: Copy> IndexMut<usize> for PodVector<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        assert!(index < self.len);
        unsafe { &mut *self.base_ptr_mut().add(index) }
    }
}

pub fn swap<T: Copy>(lhs: &mut PodVector<T>, rhs: &mut PodVector<T>) {
    lhs.swap(rhs);
}

pub fn erase_if<T: Copy, F>(vector: &mut PodVector<T>, mut pred: F) -> usize
where
    F: FnMut(&T) -> bool,
{
    let old_len = vector.len;
    let mut write = 0;
    for read in 0..old_len {
        let value = vector[read];
        if !pred(&value) {
            if write != read {
                vector[write] = value;
            }
            write += 1;
        }
    }
    vector.len = write;
    old_len - write
}

pub fn erase<T>(vector: &mut PodVector<T>, value: &T) -> usize
where
    T: Copy + PartialEq,
{
    erase_if(vector, |item| item == value)
}
