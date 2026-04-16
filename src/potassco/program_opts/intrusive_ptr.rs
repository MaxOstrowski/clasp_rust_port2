//! Port target for original_clasp/libpotassco/potassco/program_opts/intrusive_ptr.h.

use std::mem;
use std::ops::Deref;
use std::ptr::NonNull;

pub trait IntrusiveRefCounted {
    fn intrusive_add_ref(&self);
    fn intrusive_release(&self) -> i32;
    fn intrusive_count(&self) -> i32;
}

pub struct IntrusiveSharedPtr<T: IntrusiveRefCounted + ?Sized> {
    ptr: Option<NonNull<T>>,
}

impl<T: IntrusiveRefCounted> IntrusiveSharedPtr<T> {
    pub fn new(value: T) -> Self {
        let raw = Box::into_raw(Box::new(value));
        Self {
            ptr: Some(NonNull::new(raw).expect("boxed values are never null")),
        }
    }
}

impl<T: IntrusiveRefCounted + ?Sized> IntrusiveSharedPtr<T> {
    pub const fn null() -> Self {
        Self { ptr: None }
    }

    pub fn get(&self) -> Option<&T> {
        self.ptr.map(|ptr| {
            // SAFETY: `ptr` is created from a live allocation and only freed once
            // the intrusive count reaches zero.
            unsafe { ptr.as_ref() }
        })
    }

    pub fn unique(&self) -> bool {
        self.ptr.is_none_or(|ptr| {
            // SAFETY: see `get`.
            unsafe { ptr.as_ref().intrusive_count() == 1 }
        })
    }

    pub fn count(&self) -> i32 {
        self.ptr.map_or(0, |ptr| {
            // SAFETY: see `get`.
            unsafe { ptr.as_ref().intrusive_count() }
        })
    }

    pub fn reset(&mut self) {
        self.release();
    }

    pub fn swap(&mut self, other: &mut Self) {
        mem::swap(&mut self.ptr, &mut other.ptr);
    }

    fn add_ref(&self) {
        if let Some(ptr) = self.ptr {
            // SAFETY: see `get`.
            unsafe { ptr.as_ref().intrusive_add_ref() };
        }
    }

    fn release(&mut self) {
        if let Some(prev) = self.ptr.take() {
            // SAFETY: see `get`.
            let remaining = unsafe { prev.as_ref().intrusive_release() };
            if remaining == 0 {
                // SAFETY: the allocation originated from `Box::into_raw` and the
                // object reached ref-count zero exactly once.
                unsafe {
                    drop(Box::from_raw(prev.as_ptr()));
                }
            }
        }
    }
}

impl<T: IntrusiveRefCounted + ?Sized> Clone for IntrusiveSharedPtr<T> {
    fn clone(&self) -> Self {
        self.add_ref();
        Self { ptr: self.ptr }
    }
}

impl<T: IntrusiveRefCounted + ?Sized> Default for IntrusiveSharedPtr<T> {
    fn default() -> Self {
        Self::null()
    }
}

impl<T: IntrusiveRefCounted + ?Sized> Drop for IntrusiveSharedPtr<T> {
    fn drop(&mut self) {
        self.release();
    }
}

impl<T: IntrusiveRefCounted + ?Sized> Deref for IntrusiveSharedPtr<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get().expect("dereferencing a null intrusive pointer")
    }
}

pub fn make_shared<T: IntrusiveRefCounted>(value: T) -> IntrusiveSharedPtr<T> {
    IntrusiveSharedPtr::new(value)
}
