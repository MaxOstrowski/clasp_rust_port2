//! Rust port of the utility types required from
//! original_clasp/clasp/util/misc_types.h.

use crate::potassco::bits;

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
