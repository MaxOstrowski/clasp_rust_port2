//! Rust port of `original_clasp/clasp/pod_vector.h`.

use crate::clasp::util::pod_vector::PodVector as RawPodVector;
use std::borrow::Borrow;
use std::marker::PhantomData;

pub type PodVectorT<T> = RawPodVector<T>;

pub struct PodVector<T>(PhantomData<T>);

impl<T: Copy> PodVector<T> {
    pub fn destruct(vector: &mut PodVectorT<T>) {
        vector.clear();
    }
}

pub const fn to_u32(x: usize) -> u32 {
    assert!(x <= u32::MAX as usize);
    x as u32
}

pub fn size32<T, R>(range: &R) -> u32
where
    R: AsRef<[T]> + ?Sized,
{
    to_u32(range.as_ref().len())
}

pub fn discard_vec<T>(value: &mut T)
where
    T: Default,
{
    let _ = std::mem::take(value);
}

pub trait VectorLike<T>: Default {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn capacity(&self) -> usize;
    fn reserve(&mut self, new_capacity: usize);
    fn resize(&mut self, new_len: usize, value: T);
    fn truncate(&mut self, new_len: usize);
    fn as_slice(&self) -> &[T];
    fn as_mut_slice(&mut self) -> &mut [T];
}

impl<T: Copy> VectorLike<T> for RawPodVector<T> {
    fn len(&self) -> usize {
        PodVectorT::len(self)
    }

    fn capacity(&self) -> usize {
        PodVectorT::capacity(self)
    }

    fn reserve(&mut self, new_capacity: usize) {
        PodVectorT::reserve(self, new_capacity);
    }

    fn resize(&mut self, new_len: usize, value: T) {
        PodVectorT::resize(self, new_len, value);
    }

    fn truncate(&mut self, new_len: usize) {
        if new_len < self.len() {
            let _ = self.erase_range(new_len..self.len());
        }
    }

    fn as_slice(&self) -> &[T] {
        PodVectorT::as_slice(self)
    }

    fn as_mut_slice(&mut self) -> &mut [T] {
        PodVectorT::as_mut_slice(self)
    }
}

impl<T: Copy> VectorLike<T> for Vec<T> {
    fn len(&self) -> usize {
        Vec::len(self)
    }

    fn capacity(&self) -> usize {
        Vec::capacity(self)
    }

    fn reserve(&mut self, new_capacity: usize) {
        if new_capacity > self.capacity() {
            Vec::reserve(self, new_capacity - self.capacity());
        }
    }

    fn resize(&mut self, new_len: usize, value: T) {
        Vec::resize(self, new_len, value);
    }

    fn truncate(&mut self, new_len: usize) {
        Vec::truncate(self, new_len);
    }

    fn as_slice(&self) -> &[T] {
        Vec::as_slice(self)
    }

    fn as_mut_slice(&mut self) -> &mut [T] {
        Vec::as_mut_slice(self)
    }
}

pub fn shrink_vec_to<V, T>(vector: &mut V, new_len: usize)
where
    V: VectorLike<T>,
{
    vector.truncate(new_len);
}

pub fn grow_vec_to<V, T>(vector: &mut V, new_len: usize, value: T)
where
    V: VectorLike<T>,
    T: Copy,
{
    if vector.len() < new_len {
        if vector.capacity() < new_len {
            vector.reserve(new_len + new_len / 2);
        }
        vector.resize(new_len, value);
    }
}

pub fn move_down<V, T>(vector: &mut V, from: usize, to: usize)
where
    V: VectorLike<T>,
    T: Copy,
{
    let len = vector.len();
    let slice = vector.as_mut_slice();
    let mut read = from;
    let mut write = to;
    while read != len {
        slice[write] = slice[read];
        write += 1;
        read += 1;
    }
    shrink_vec_to(vector, write);
}

pub fn contains<I, V>(range: I, value: &V) -> bool
where
    I: IntoIterator,
    I::Item: Borrow<V>,
    V: PartialEq + ?Sized,
{
    range.into_iter().any(|item| item.borrow() == value)
}

pub fn drop<T, R>(range: &R, offset: usize) -> &[T]
where
    R: AsRef<[T]> + ?Sized,
{
    let slice = range.as_ref();
    assert!(offset <= slice.len());
    &slice[offset..]
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PodQueue<T: Copy> {
    pub vec: PodVectorT<T>,
    pub q_front: u32,
}

impl<T: Copy> PodQueue<T> {
    #[must_use]
    pub fn empty(&self) -> bool {
        self.q_front == size32(&self.vec)
    }

    #[must_use]
    pub fn size(&self) -> u32 {
        size32(&self.vec) - self.q_front
    }

    #[must_use]
    pub fn front(&self) -> &T {
        &self.vec[self.q_front as usize]
    }

    #[must_use]
    pub fn back(&self) -> &T {
        self.vec.back()
    }

    pub fn front_mut(&mut self) -> &mut T {
        &mut self.vec[self.q_front as usize]
    }

    pub fn back_mut(&mut self) -> &mut T {
        self.vec.back_mut()
    }

    pub fn push(&mut self, value: T) {
        self.vec.push_back(value);
    }

    pub fn pop(&mut self) {
        self.q_front += 1;
    }

    pub fn pop_ret(&mut self) -> T {
        let value = self.vec[self.q_front as usize];
        self.q_front += 1;
        value
    }

    pub fn rewind(&mut self) {
        self.q_front = 0;
    }

    pub fn clear(&mut self) {
        self.vec.clear();
        self.q_front = 0;
    }
}
