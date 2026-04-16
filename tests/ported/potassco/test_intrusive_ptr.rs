//! Port target for original_clasp/libpotassco/potassco/program_opts/intrusive_ptr.h.

use std::cell::Cell;
use std::mem;
use std::rc::Rc;

use rust_clasp::potassco::program_opts::{IntrusiveRefCounted, IntrusiveSharedPtr, make_shared};

struct Foo {
    x: Cell<i32>,
    live_count: Rc<Cell<i32>>,
    rc: Cell<i32>,
}

impl Foo {
    fn new(live_count: Rc<Cell<i32>>) -> Self {
        live_count.set(live_count.get() + 1);
        Self {
            x: Cell::new(12),
            live_count,
            rc: Cell::new(1),
        }
    }
}

impl Drop for Foo {
    fn drop(&mut self) {
        self.live_count.set(self.live_count.get() - 1);
    }
}

impl IntrusiveRefCounted for Foo {
    fn intrusive_add_ref(&self) {
        self.rc.set(self.rc.get() + 1);
    }

    fn intrusive_release(&self) -> i32 {
        let next = self.rc.get() - 1;
        self.rc.set(next);
        next
    }

    fn intrusive_count(&self) -> i32 {
        self.rc.get()
    }
}

#[test]
fn intrusive_pointer_copy_assignment_matches_upstream_behavior() {
    let count = Rc::new(Cell::new(0));
    let ptr = make_shared(Foo::new(count.clone()));

    assert_eq!(ptr.count(), 1);
    assert!(ptr.unique());
    assert_eq!(count.get(), 1);

    {
        let mut ptr2 = ptr.clone();
        assert_eq!(ptr2.count(), 2);

        let mut ptr3: IntrusiveSharedPtr<Foo> = IntrusiveSharedPtr::default();
        assert!(ptr3.get().is_none());
        ptr3 = ptr2.clone();
        assert_eq!(ptr3.count(), 3);
        assert!(!ptr3.unique());

        ptr2 = ptr3.clone();
        assert_eq!(ptr2.count(), 3);
        ptr2.x.set(77);
    }

    assert_eq!(count.get(), 1);
    assert_eq!(ptr.x.get(), 77);
}

#[test]
fn intrusive_pointer_move_and_reset_match_upstream_behavior() {
    let count = Rc::new(Cell::new(0));
    let mut ptr = make_shared(Foo::new(count.clone()));

    let mut ptr2 = mem::take(&mut ptr);
    assert_eq!(ptr2.count(), 1);
    assert!(ptr.get().is_none());

    let mut ptr3: IntrusiveSharedPtr<Foo> = IntrusiveSharedPtr::default();
    assert!(ptr3.get().is_none());
    ptr3 = mem::take(&mut ptr2);
    assert_eq!(ptr3.count(), 1);
    assert!(ptr2.get().is_none());

    ptr = mem::take(&mut ptr3);
    assert!(ptr.get().is_some());
    ptr.x.set(77);

    assert_eq!(count.get(), 1);
    assert_eq!(ptr.x.get(), 77);

    ptr.reset();
    assert_eq!(count.get(), 0);
    assert!(ptr.unique());
}
