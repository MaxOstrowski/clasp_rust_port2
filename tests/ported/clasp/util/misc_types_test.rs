use rust_clasp::clasp::util::misc_types::{
    EnterEvent, Event, EventLike, LockedValue, MovingAvg, MovingAvgType, Range32, RefCount, Rng,
    SigAtomic, Subsystem, TaggedPtr, Verbosity, choose, clamp, event_cast, event_id, percent,
    ratio, ratio_with_default, saturate_cast,
};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[test]
fn choose_and_ratio_follow_upstream_behavior() {
    assert_eq!(choose(5, 2), 10);
    assert_eq!(choose(5, 7), 0);
    assert_eq!(choose(10, 8), choose(10, 2));
    assert_eq!(ratio(9, 0), 0.0);
    assert_eq!(ratio_with_default(9, 0, 1.5), 1.5);
    assert_eq!(percent(1, 4), 25.0);
}

#[test]
fn rng_produces_the_upstream_sequence() {
    let mut rng = Rng::new(1);
    let expected = [
        41, 18_467, 6_334, 26_500, 19_169, 15_724, 11_478, 29_358, 26_962, 24_464,
    ];
    for expected_value in expected {
        assert_eq!(rng.rand(), expected_value);
    }
}

#[test]
fn rng_drand_irand_and_shuffle_match_upstream_algorithm() {
    let mut rng = Rng::new(1);
    assert_eq!(rng.drand(), 0.001_251_220_703_125);
    assert_eq!(rng.irand(100), 56);

    let mut shuffled = [0, 1, 2, 3, 4, 5];
    let mut rng = Rng::new(1);
    rng.shuffle(&mut shuffled);
    assert_eq!(shuffled, [3, 2, 0, 5, 4, 1]);
}

#[test]
fn moving_avg_tracks_simple_moving_average() {
    let mut avg = MovingAvg::new(3, MovingAvgType::AvgSma);
    assert!(!avg.push(10));
    assert_eq!(avg.get(), 10.0);
    assert!(!avg.push(20));
    assert_eq!(avg.get(), 15.0);
    assert!(avg.push(5));
    assert_eq!(avg.get(), 35.0 / 3.0);
    assert!(avg.push(30));
    assert_eq!(avg.get(), 55.0 / 3.0);
}

#[test]
fn moving_avg_tracks_ema_variants() {
    let mut ema = MovingAvg::new(4, MovingAvgType::AvgEma);
    assert!(!ema.push(10));
    assert_eq!(ema.get(), 10.0);
    assert!(!ema.push(20));
    assert_eq!(ema.get(), 15.0);
    assert!(!ema.push(5));
    assert_eq!(ema.get(), 35.0 / 3.0);
    assert!(ema.push(30));
    assert_eq!(ema.get(), 16.25);

    let mut smooth = MovingAvg::new(4, MovingAvgType::AvgEmaSmooth);
    assert!(!smooth.push(10));
    assert!(!smooth.push(20));
    assert!(!smooth.push(5));
    assert_eq!(smooth.get(), 11.0);
    assert!(smooth.push(30));
    assert_eq!(smooth.get(), 18.6);

    let mut log_smooth = MovingAvg::new(4, MovingAvgType::AvgEmaLogSmooth);
    assert!(!log_smooth.push(10));
    assert!(!log_smooth.push(20));
    assert!(!log_smooth.push(5));
    assert_eq!(log_smooth.get(), 12.5);
    assert!(log_smooth.push(30));
    assert_eq!(log_smooth.get(), 16.875);
}

#[test]
fn moving_avg_window_zero_behaves_like_cumulative_average() {
    let mut avg = MovingAvg::new(0, MovingAvgType::AvgSma);
    assert!(avg.valid());
    assert!(avg.push(10));
    assert_eq!(avg.get(), 10.0);
    assert!(avg.push(20));
    assert_eq!(avg.get(), 15.0);
    avg.clear();
    assert!(avg.valid());
    assert_eq!(avg.get(), 0.0);
    assert!(avg.push(30));
    assert_eq!(avg.get(), 30.0);
}

#[test]
fn clamp_and_saturate_cast_follow_upstream_behavior() {
    assert_eq!(clamp(7, 1, 5), 5);
    assert_eq!(clamp(-3, -2, 4), -2);
    assert_eq!(saturate_cast::<i16, _>(123_i32), 123);
    assert_eq!(saturate_cast::<i16, _>(40_000_i32), i16::MAX);
    assert_eq!(saturate_cast::<u16, _>(-7_i32), 0);
    assert_eq!(saturate_cast::<u8, _>(999_u32), u8::MAX);
}

#[test]
fn tagged_ptr_tracks_pointer_and_tag_bits() {
    #[repr(align(8))]
    struct Aligned(u32);

    let mut value = Box::new(Aligned(17));
    let ptr = value.as_mut() as *mut Aligned;
    let mut tagged = TaggedPtr::<Aligned, 2>::new(ptr);
    assert_eq!(tagged.get(), ptr);
    assert!(!tagged.any());

    tagged.set::<0>();
    assert!(tagged.test::<0>());
    assert!(tagged.any());

    tagged.toggle::<1>();
    assert!(tagged.test::<1>());
    tagged.clear::<0>();
    assert!(!tagged.test::<0>());
    assert_eq!(unsafe { (*tagged.get()).0 }, 17);

    let all = TaggedPtr::<Aligned, 2>::with_all_tags(ptr);
    assert!(all.test::<0>());
    assert!(all.test::<1>());
}

#[test]
fn range32_orders_bounds_and_clamps_values() {
    let range = Range32::new(9, 3);
    assert_eq!(range.lo, 3);
    assert_eq!(range.hi, 9);
    assert_eq!(range.clamp(1), 3);
    assert_eq!(range.clamp(7), 7);
    assert_eq!(range.clamp(12), 9);
}

#[test]
fn event_ids_are_stable_and_casts_match_runtime_type() {
    #[derive(Debug)]
    struct DemoEvent {
        base: Event,
        value: u32,
    }

    impl DemoEvent {
        fn new(value: u32) -> Self {
            Self {
                base: Event::for_type::<Self>(Subsystem::SubsystemSolve, Verbosity::VerbosityLow),
                value,
            }
        }
    }

    impl EventLike for DemoEvent {
        fn event(&self) -> &Event {
            &self.base
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    let first = event_id::<DemoEvent>();
    let second = event_id::<DemoEvent>();
    assert_eq!(first, second);

    let enter = EnterEvent::new(Subsystem::SubsystemLoad, Verbosity::VerbosityHigh);
    let demo = DemoEvent::new(42);
    assert!(event_cast::<DemoEvent>(&demo).is_some());
    assert!(event_cast::<EnterEvent>(&demo).is_none());
    assert!(event_cast::<EnterEvent>(&enter).is_some());
    assert_eq!(demo.value, 42);
}

#[test]
fn refcount_matches_upstream_release_sequence() {
    let count = RefCount::new(4);
    assert_eq!(count.count(), 4);
    assert!(!count.release(2));
    assert_eq!(count.count(), 2);
    count.add(1);
    assert_eq!(count.count(), 3);
    assert!(!count.release(1));
    assert_eq!(count.count(), 2);
    assert_eq!(count.release_fetch(1), 1);
    assert!(count.release(1));
}

#[test]
fn sig_atomic_matches_upstream_exchange_and_set_if_unset_behavior() {
    let sig = SigAtomic::new();
    assert_eq!(sig.value(), 0);
    assert_eq!(sig.exchange(12), 0);
    assert_eq!(sig.value(), 12);
    assert!(!sig.set_if_unset(1));
    assert_eq!(sig.value(), 12);
    assert_eq!(sig.exchange(0), 12);
    assert!(sig.set_if_unset(1));
    assert_eq!(sig.value(), 1);
}

#[test]
fn locked_value_uint_matches_upstream_locking_pattern() {
    let value = Arc::new(LockedValue::new(0_u32));
    assert!(value.try_lock());
    assert!(!value.try_lock());
    assert_eq!(value.value(), 0);

    let expected = Arc::new(Mutex::new(0x0CAFEA00_u32));
    let mut threads = Vec::new();
    for id in 1_u32..64 {
        let worker_value = Arc::clone(&value);
        let worker_expected = Arc::clone(&expected);
        threads.push(thread::spawn(move || {
            thread::sleep(Duration::from_nanos((100 - id) as u64));
            let current = worker_value.lock();
            let mut expected_guard = worker_expected.lock().expect("expected mutex poisoned");
            assert_eq!(current, *expected_guard);
            *expected_guard += id;
            worker_value.store_unlock(*expected_guard);
        }));
        if id == 20 {
            let current = *expected.lock().expect("expected mutex poisoned");
            value.store_unlock(current);
        }
    }

    for thread in threads {
        thread.join().expect("worker thread panicked");
    }

    let expected = *expected.lock().expect("expected mutex poisoned");
    assert_eq!(expected, 0x0CAFF1E0);
    assert_eq!(value.value(), expected);
}

#[test]
fn locked_value_pointer_matches_upstream_locking_pattern() {
    let value = Arc::new(LockedValue::<*mut u32>::new(ptr::null_mut()));
    assert!(value.try_lock());
    assert!(value.value().is_null());

    let active = Arc::new(Mutex::new(Some(Box::new(0x0CAFEA00_u32))));
    let mut threads = Vec::new();
    for id in 1_u32..64 {
        let worker_value = Arc::clone(&value);
        let worker_active = Arc::clone(&active);
        threads.push(thread::spawn(move || {
            thread::sleep(Duration::from_nanos((100 - id) as u64));
            let current = worker_value.lock();
            let mut active_guard = worker_active.lock().expect("active mutex poisoned");
            let current_box = active_guard.as_ref().expect("active value missing");
            let current_ptr = (&**current_box) as *const u32 as *mut u32;
            assert_eq!(current, current_ptr);
            let next = Box::new(*current_box.as_ref() + id);
            let next_ptr = (&*next) as *const u32 as *mut u32;
            *active_guard = Some(next);
            worker_value.store_unlock(next_ptr);
        }));
        if id == 17 {
            let active_guard = active.lock().expect("active mutex poisoned");
            let ptr = active_guard
                .as_ref()
                .map(|value| (&**value) as *const u32 as *mut u32)
                .expect("active value missing");
            drop(active_guard);
            value.store_unlock(ptr);
        }
    }

    for thread in threads {
        thread.join().expect("worker thread panicked");
    }

    let active_guard = active.lock().expect("active mutex poisoned");
    let final_value = active_guard.as_ref().expect("active value missing");
    assert_eq!(**final_value, 0x0CAFF1E0);
}
