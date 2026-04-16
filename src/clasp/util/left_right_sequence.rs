//! Rust port of `original_clasp/clasp/util/left_right_sequence.h`.

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::marker::PhantomData;
use std::mem::{ManuallyDrop, MaybeUninit, align_of, size_of};
use std::ptr::{self, NonNull};

#[repr(C)]
union AlignAs<L, R> {
    left: ManuallyDrop<MaybeUninit<L>>,
    right: ManuallyDrop<MaybeUninit<R>>,
}

#[repr(C)]
struct InlineStorage<L, R, const I: usize> {
    _align: [MaybeUninit<AlignAs<L, R>>; 0],
    bytes: [MaybeUninit<u8>; I],
}

impl<L, R, const I: usize> InlineStorage<L, R, I> {
    const fn new() -> Self {
        Self {
            _align: [],
            bytes: [MaybeUninit::uninit(); I],
        }
    }

    fn as_ptr(&self) -> *const u8 {
        self.bytes.as_ptr().cast::<u8>()
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.bytes.as_mut_ptr().cast::<u8>()
    }
}

#[repr(C)]
struct HeaderModel {
    ptr: *const u8,
    cap_and_flag: u32,
    left: u32,
    right: u32,
}

const HEAP_FLAG: u32 = 1 << 31;

const fn max_usize(lhs: usize, rhs: usize) -> usize {
    if lhs >= rhs { lhs } else { rhs }
}

const fn align_up(value: usize, align: usize) -> usize {
    value.div_ceil(align) * align
}

const fn block_size<L, R>() -> usize {
    let max_size = max_usize(size_of::<L>(), size_of::<R>());
    let max_align = max_usize(align_of::<L>(), align_of::<R>());
    align_up(max_size, max_align)
}

pub const fn compute_raw_cap<L, R, const I: usize>() -> usize {
    let bs = block_size::<L, R>();
    let header = size_of::<HeaderModel>();
    if bs == 0 || header + bs > I {
        0
    } else {
        ((I - header) / bs) * bs
    }
}

pub struct RightIter<'a, R> {
    front: *const R,
    back: *const R,
    remaining: usize,
    marker: PhantomData<&'a R>,
}

impl<'a, R> Iterator for RightIter<'a, R> {
    type Item = &'a R;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.front = self.front.wrapping_sub(1);
        self.remaining -= 1;
        // SAFETY: `front` and `back` delimit initialized elements inside the
        // backing storage while `remaining > 0`.
        Some(unsafe { &*self.front })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, R> DoubleEndedIterator for RightIter<'a, R> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let current = self.back;
        self.back = self.back.wrapping_add(1);
        self.remaining -= 1;
        // SAFETY: `back` advances only through initialized elements in the
        // backing storage while `remaining > 0`.
        Some(unsafe { &*current })
    }
}

impl<'a, R> ExactSizeIterator for RightIter<'a, R> {}

/// Stores two sequences in one contiguous memory block.
///
/// This port keeps the original byte-level layout strategy: left values grow
/// from the start of the buffer and right values grow from the end. The const
/// parameter `I` is interpreted like the upstream stack budget and translated
/// into an inline raw byte capacity via `compute_raw_cap`.
pub struct LeftRightSequence<L: Copy, R: Copy, const I: usize> {
    heap: Option<NonNull<u8>>,
    cap_and_flag: u32,
    left_bytes: u32,
    right_offset: u32,
    inline: InlineStorage<L, R, I>,
}

impl<L: Copy, R: Copy, const I: usize> LeftRightSequence<L, R, I> {
    pub const BLOCK_SIZE: usize = block_size::<L, R>();
    pub const INLINE_RAW_CAP: usize = compute_raw_cap::<L, R, I>();

    pub fn new() -> Self {
        assert!(
            size_of::<L>() > 0,
            "LeftRightSequence does not support ZST left values"
        );
        assert!(
            size_of::<R>() > 0,
            "LeftRightSequence does not support ZST right values"
        );
        let mut this = Self {
            heap: None,
            cap_and_flag: 0,
            left_bytes: 0,
            right_offset: 0,
            inline: InlineStorage::new(),
        };
        this.init_inline();
        this
    }

    pub fn empty(&self) -> bool {
        self.left_bytes == 0 && self.right_offset == self.cap_bytes_u32()
    }

    pub fn size(&self) -> usize {
        self.left_size() + self.right_size()
    }

    pub fn left_size(&self) -> usize {
        self.left_bytes as usize / size_of::<L>()
    }

    pub fn right_size(&self) -> usize {
        self.right_size_bytes() / size_of::<R>()
    }

    pub fn left_capacity(&self) -> usize {
        self.cap_bytes() / size_of::<L>()
    }

    pub fn right_capacity(&self) -> usize {
        self.cap_bytes() / size_of::<R>()
    }

    pub fn left_view(&self) -> &[L] {
        // SAFETY: the left region is always aligned, initialized, and exactly
        // `left_size()` elements long.
        unsafe { std::slice::from_raw_parts(self.base_ptr().cast::<L>(), self.left_size()) }
    }

    pub fn right_view(&self) -> RightIter<'_, R> {
        let base = self.right_base_ptr();
        RightIter {
            front: self.end_ptr().cast::<R>(),
            back: base,
            remaining: self.right_size(),
            marker: PhantomData,
        }
    }

    pub fn left(&self, index: usize) -> &L {
        &self.left_view()[index]
    }

    pub fn left_mut(&mut self, index: usize) -> &mut L {
        assert!(index < self.left_size());
        // SAFETY: `index` is within the initialized left region.
        unsafe { &mut *self.base_mut_ptr().cast::<L>().add(index) }
    }

    pub fn right(&self, index: usize) -> &R {
        assert!(index < self.right_size());
        // SAFETY: logical right index `index` maps to an initialized element in
        // the right region.
        unsafe { &*self.end_ptr().cast::<R>().sub(index + 1) }
    }

    pub fn right_mut(&mut self, index: usize) -> &mut R {
        assert!(index < self.right_size());
        // SAFETY: logical right index `index` maps to an initialized element in
        // the right region.
        unsafe { &mut *self.end_mut_ptr().cast::<R>().sub(index + 1) }
    }

    pub fn push_left(&mut self, value: L) {
        self.ensure_space(size_of::<L>());
        // SAFETY: enough space was reserved and the target slot is aligned.
        unsafe {
            self.base_mut_ptr()
                .add(self.left_bytes as usize)
                .cast::<L>()
                .write(value);
        }
        self.left_bytes += size_of::<L>() as u32;
    }

    pub fn push_right(&mut self, value: R) {
        self.ensure_space(size_of::<R>());
        self.right_offset -= size_of::<R>() as u32;
        // SAFETY: enough space was reserved and the target slot is aligned.
        unsafe {
            self.base_mut_ptr()
                .add(self.right_offset as usize)
                .cast::<R>()
                .write(value);
        }
    }

    pub fn pop_left(&mut self) {
        assert!(self.left_size() != 0);
        self.left_bytes -= size_of::<L>() as u32;
    }

    pub fn pop_right(&mut self) {
        assert!(self.right_size() != 0);
        self.right_offset += size_of::<R>() as u32;
    }

    pub fn erase_left(&mut self, index: usize) {
        let len = self.left_size();
        assert!(index < len);
        // SAFETY: source and destination stay within the initialized left region.
        unsafe {
            let dst = self.base_mut_ptr().cast::<L>().add(index);
            ptr::copy(dst.add(1), dst, len - index - 1);
        }
        self.left_bytes -= size_of::<L>() as u32;
    }

    pub fn erase_left_unordered(&mut self, index: usize) {
        let len = self.left_size();
        assert!(index < len);
        if index + 1 != len {
            // SAFETY: both pointers reference initialized elements in the left region.
            unsafe {
                let base = self.base_mut_ptr().cast::<L>();
                ptr::copy_nonoverlapping(base.add(len - 1), base.add(index), 1);
            }
        }
        self.left_bytes -= size_of::<L>() as u32;
    }

    pub fn erase_right(&mut self, index: usize) {
        let len = self.right_size();
        assert!(index < len);
        let physical_index = len - 1 - index;
        // SAFETY: the copied range stays within the initialized right region and
        // `ptr::copy` supports overlap.
        unsafe {
            let base = self.right_base_mut_ptr();
            ptr::copy(base, base.add(1), physical_index);
        }
        self.right_offset += size_of::<R>() as u32;
    }

    pub fn erase_right_unordered(&mut self, index: usize) {
        let len = self.right_size();
        assert!(index < len);
        let physical_index = len - 1 - index;
        // SAFETY: both pointers are within the initialized right region.
        unsafe {
            let base = self.right_base_mut_ptr();
            ptr::copy_nonoverlapping(base, base.add(physical_index), 1);
        }
        self.right_offset += size_of::<R>() as u32;
    }

    pub fn shrink_left_to(&mut self, new_len: usize) {
        assert!(new_len <= self.left_size());
        self.left_bytes = (new_len * size_of::<L>()) as u32;
    }

    pub fn shrink_right_to(&mut self, new_len: usize) {
        assert!(new_len <= self.right_size());
        self.right_offset = (self.cap_bytes() - new_len * size_of::<R>()) as u32;
    }

    pub fn clear(&mut self) {
        self.left_bytes = 0;
        self.right_offset = self.cap_bytes_u32();
    }

    pub fn reset(&mut self) {
        self.release_heap();
        self.init_inline();
    }

    pub fn try_shrink(&mut self) {
        let inline_cap = Self::INLINE_RAW_CAP;
        if inline_cap == 0 || !self.is_heap() || self.raw_size_bytes() > inline_cap {
            return;
        }
        let right_bytes = self.right_size_bytes();
        let left_bytes = self.left_bytes as usize;
        let heap_ptr = self.base_ptr();
        let inline_ptr = self.inline.as_mut_ptr();
        // SAFETY: the source regions are initialized and non-overlapping in the
        // heap buffer, and the inline target has sufficient aligned capacity.
        unsafe {
            ptr::copy_nonoverlapping(heap_ptr, inline_ptr, left_bytes);
            ptr::copy_nonoverlapping(
                heap_ptr.add(self.right_offset as usize),
                inline_ptr.add(inline_cap - right_bytes),
                right_bytes,
            );
        }
        self.release_heap();
        self.cap_and_flag = inline_cap as u32;
        self.right_offset = (inline_cap - right_bytes) as u32;
    }

    fn init_inline(&mut self) {
        self.heap = None;
        self.cap_and_flag = Self::INLINE_RAW_CAP as u32;
        self.left_bytes = 0;
        self.right_offset = self.cap_and_flag;
    }

    fn cap_bytes(&self) -> usize {
        self.cap_bytes_u32() as usize
    }

    fn cap_bytes_u32(&self) -> u32 {
        self.cap_and_flag & !HEAP_FLAG
    }

    fn is_heap(&self) -> bool {
        (self.cap_and_flag & HEAP_FLAG) != 0
    }

    fn raw_size_bytes(&self) -> usize {
        self.left_bytes as usize + self.right_size_bytes()
    }

    fn right_size_bytes(&self) -> usize {
        self.cap_bytes() - self.right_offset as usize
    }

    fn capacity_blocks(&self) -> usize {
        self.cap_bytes() / Self::BLOCK_SIZE
    }

    fn base_ptr(&self) -> *const u8 {
        match self.heap {
            Some(ptr) => ptr.as_ptr(),
            None => self.inline.as_ptr(),
        }
    }

    fn base_mut_ptr(&mut self) -> *mut u8 {
        match self.heap {
            Some(ptr) => ptr.as_ptr(),
            None => self.inline.as_mut_ptr(),
        }
    }

    fn end_ptr(&self) -> *const u8 {
        self.base_ptr().wrapping_add(self.cap_bytes())
    }

    fn end_mut_ptr(&mut self) -> *mut u8 {
        self.base_mut_ptr().wrapping_add(self.cap_bytes())
    }

    fn right_base_ptr(&self) -> *const R {
        self.base_ptr()
            .wrapping_add(self.right_offset as usize)
            .cast::<R>()
    }

    fn right_base_mut_ptr(&mut self) -> *mut R {
        self.base_mut_ptr()
            .wrapping_add(self.right_offset as usize)
            .cast::<R>()
    }

    fn ensure_space(&mut self, additional_bytes: usize) {
        let needed = self.raw_size_bytes() + additional_bytes;
        if needed > self.cap_bytes() {
            self.realloc(needed);
        }
    }

    fn realloc(&mut self, required: usize) {
        let mut new_cap = ((self.capacity_blocks() * 3) >> 1) * Self::BLOCK_SIZE;
        let min_cap = 4 * Self::BLOCK_SIZE;
        if new_cap < min_cap {
            new_cap = min_cap;
        }
        let required = align_up(required, Self::BLOCK_SIZE);
        if new_cap < required {
            new_cap = required;
        }

        let layout = Self::layout_for(new_cap);
        // SAFETY: the layout is valid and non-zero in size because `required`
        // exceeded the current capacity.
        let new_ptr = unsafe { alloc(layout) };
        if new_ptr.is_null() {
            handle_alloc_error(layout);
        }

        let right_bytes = self.right_size_bytes();
        let left_bytes = self.left_bytes as usize;
        // SAFETY: the new allocation has sufficient capacity and alignment.
        unsafe {
            ptr::copy_nonoverlapping(self.base_ptr(), new_ptr, left_bytes);
            ptr::copy_nonoverlapping(
                self.base_ptr().add(self.right_offset as usize),
                new_ptr.add(new_cap - right_bytes),
                right_bytes,
            );
        }

        self.release_heap();
        self.heap = Some(NonNull::new(new_ptr).expect("allocator returned null"));
        self.cap_and_flag = (new_cap as u32) | HEAP_FLAG;
        self.right_offset = (new_cap - right_bytes) as u32;
    }

    fn release_heap(&mut self) {
        if let Some(ptr) = self.heap.take() {
            let layout = Self::layout_for(self.cap_bytes());
            // SAFETY: `ptr` was allocated with the same layout in `realloc`.
            unsafe { dealloc(ptr.as_ptr(), layout) };
        }
        self.cap_and_flag &= !HEAP_FLAG;
    }

    fn layout_for(capacity: usize) -> Layout {
        Layout::from_size_align(capacity, max_usize(align_of::<L>(), align_of::<R>()))
            .expect("valid left_right_sequence allocation layout")
    }
}

impl<L: Copy, R: Copy, const I: usize> Clone for LeftRightSequence<L, R, I> {
    fn clone(&self) -> Self {
        let mut copy = Self::new();
        let raw_size = self.raw_size_bytes();
        let mut new_cap = Self::INLINE_RAW_CAP;
        if raw_size > new_cap {
            new_cap = align_up(raw_size, Self::BLOCK_SIZE);
            let layout = Self::layout_for(new_cap);
            // SAFETY: `new_cap` is rounded to the container block size and is
            // non-zero whenever allocation is required.
            let new_ptr = unsafe { alloc(layout) };
            if new_ptr.is_null() {
                handle_alloc_error(layout);
            }
            copy.heap = Some(NonNull::new(new_ptr).expect("allocator returned null"));
            copy.cap_and_flag = (new_cap as u32) | HEAP_FLAG;
        }
        copy.left_bytes = self.left_bytes;
        copy.right_offset = (new_cap - self.right_size_bytes()) as u32;
        // SAFETY: both copy targets are valid initialized regions with enough capacity.
        unsafe {
            ptr::copy_nonoverlapping(
                self.base_ptr(),
                copy.base_mut_ptr(),
                self.left_bytes as usize,
            );
            ptr::copy_nonoverlapping(
                self.base_ptr().add(self.right_offset as usize),
                copy.base_mut_ptr().add(copy.right_offset as usize),
                self.right_size_bytes(),
            );
        }
        copy
    }
}

impl<L: Copy, R: Copy, const I: usize> Default for LeftRightSequence<L, R, I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<L: Copy, R: Copy, const I: usize> Drop for LeftRightSequence<L, R, I> {
    fn drop(&mut self) {
        self.release_heap();
    }
}
