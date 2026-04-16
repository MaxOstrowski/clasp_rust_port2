use rust_clasp::clasp::util::misc_types::{
    MovingAvg, MovingAvgType, Rng, choose, percent, ratio, ratio_with_default,
};

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
