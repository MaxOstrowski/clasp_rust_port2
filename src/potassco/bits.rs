//! Rust port of original_clasp/libpotassco/potassco/bits.h.

use core::cmp::Ordering;
use core::marker::PhantomData;
use core::ops::{BitAnd, BitOr, BitXor, Not, Shl, Sub};

pub trait UnsignedInt:
    Copy
    + Default
    + Eq
    + Ord
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
    + BitXor<Output = Self>
    + Not<Output = Self>
    + Shl<u32, Output = Self>
    + Sub<Output = Self>
{
    const BITS: u32;

    fn one() -> Self;
    fn max_value() -> Self;
    fn bit_ceil(self) -> Self;
    fn bit_floor(self) -> Self;
    fn bit_width(self) -> u32;
    fn leading_ones(self) -> u32;
    fn wrapping_neg(self) -> Self;
    fn leading_zeros(self) -> u32;
    fn trailing_ones(self) -> u32;
    fn trailing_zeros(self) -> u32;
    fn count_ones(self) -> u32;
    fn is_power_of_two(self) -> bool;
    fn rotate_left(self, n: u32) -> Self;
    fn rotate_right(self, n: u32) -> Self;
}

macro_rules! impl_unsigned_int {
	($($ty:ty),+ $(,)?) => {
		$(
			impl UnsignedInt for $ty {
				const BITS: u32 = <$ty>::BITS;

				fn one() -> Self {
					1
				}

				fn max_value() -> Self {
					<$ty>::MAX
				}

                fn bit_ceil(self) -> Self {
                    self.next_power_of_two()
                }

                fn bit_floor(self) -> Self {
                    if self == 0 {
                        0
                    } else {
                        1 << (<$ty>::BITS - 1 - self.leading_zeros())
                    }
                }

                fn bit_width(self) -> u32 {
                    <$ty>::BITS - self.leading_zeros()
                }

                fn leading_ones(self) -> u32 {
                    self.leading_ones()
                }

				fn wrapping_neg(self) -> Self {
					self.wrapping_neg()
				}

				fn leading_zeros(self) -> u32 {
					self.leading_zeros()
				}

                fn trailing_ones(self) -> u32 {
                    self.trailing_ones()
                }

                fn trailing_zeros(self) -> u32 {
                    self.trailing_zeros()
                }

				fn count_ones(self) -> u32 {
					self.count_ones()
				}

                fn is_power_of_two(self) -> bool {
                    self.is_power_of_two()
                }

                fn rotate_left(self, n: u32) -> Self {
                    self.rotate_left(n)
                }

                fn rotate_right(self, n: u32) -> Self {
                    self.rotate_right(n)
                }
			}
		)+
	};
}

impl_unsigned_int!(u8, u16, u32, u64, u128, usize);

pub fn bit_ceil<T: UnsignedInt>(x: T) -> T {
    x.bit_ceil()
}

pub fn bit_floor<T: UnsignedInt>(x: T) -> T {
    x.bit_floor()
}

pub fn bit_width<T: UnsignedInt>(x: T) -> u32 {
    x.bit_width()
}

pub fn countl_one<T: UnsignedInt>(x: T) -> u32 {
    x.leading_ones()
}

pub fn countl_zero<T: UnsignedInt>(x: T) -> u32 {
    x.leading_zeros()
}

pub fn countr_one<T: UnsignedInt>(x: T) -> u32 {
    x.trailing_ones()
}

pub fn countr_zero<T: UnsignedInt>(x: T) -> u32 {
    x.trailing_zeros()
}

pub fn has_single_bit<T: UnsignedInt>(x: T) -> bool {
    x.is_power_of_two()
}

pub fn popcount<T: UnsignedInt>(x: T) -> u32 {
    x.count_ones()
}

pub fn rotl<T: UnsignedInt>(x: T, n: u32) -> T {
    x.rotate_left(n)
}

pub fn rotr<T: UnsignedInt>(x: T, n: u32) -> T {
    x.rotate_right(n)
}

pub fn nth_bit<T: UnsignedInt>(n: u32) -> T {
    assert!(n < T::BITS);
    T::one() << n
}

pub fn test_bit<T: UnsignedInt>(x: T, n: u32) -> bool {
    (x & nth_bit::<T>(n)) != T::default()
}

pub fn set_bit<T: UnsignedInt>(x: T, n: u32) -> T {
    x | nth_bit::<T>(n)
}

pub fn store_set_bit<T: UnsignedInt>(x: &mut T, n: u32) -> T {
    *x = set_bit(*x, n);
    *x
}

pub fn clear_bit<T: UnsignedInt>(x: T, n: u32) -> T {
    x & !nth_bit::<T>(n)
}

pub fn store_clear_bit<T: UnsignedInt>(x: &mut T, n: u32) -> T {
    *x = clear_bit(*x, n);
    *x
}

pub fn toggle_bit<T: UnsignedInt>(x: T, n: u32) -> T {
    x ^ nth_bit::<T>(n)
}

pub fn store_toggle_bit<T: UnsignedInt>(x: &mut T, n: u32) -> T {
    *x = toggle_bit(*x, n);
    *x
}

pub fn test_mask<T: UnsignedInt>(x: T, mask: T) -> bool {
    (x & mask) == mask
}

pub fn test_any<T: UnsignedInt>(x: T, mask: T) -> bool {
    (x & mask) != T::default()
}

pub fn set_mask<T: UnsignedInt>(x: T, mask: T) -> T {
    x | mask
}

pub fn store_set_mask<T: UnsignedInt>(x: &mut T, mask: T) -> T {
    *x = set_mask(*x, mask);
    *x
}

pub fn clear_mask<T: UnsignedInt>(x: T, mask: T) -> T {
    x & !mask
}

pub fn store_clear_mask<T: UnsignedInt>(x: &mut T, mask: T) -> T {
    *x = clear_mask(*x, mask);
    *x
}

pub fn toggle_mask<T: UnsignedInt>(x: T, mask: T) -> T {
    x ^ mask
}

pub fn store_toggle_mask<T: UnsignedInt>(x: &mut T, mask: T) -> T {
    *x = toggle_mask(*x, mask);
    *x
}

pub fn bit_max<T: UnsignedInt>(num_bits: u32) -> T {
    if num_bits < T::BITS {
        nth_bit::<T>(num_bits) - T::one()
    } else {
        T::max_value()
    }
}

pub fn right_most_bit<T: UnsignedInt>(x: T) -> T {
    x & x.wrapping_neg()
}

pub fn left_most_bit<T: UnsignedInt>(x: T) -> T {
    bit_floor(x)
}

pub fn log2<T: UnsignedInt>(x: T) -> u32 {
    bit_width(x) - u32::from(x != T::default())
}

pub fn bit_count<T: UnsignedInt>(x: T) -> u32 {
    popcount(x)
}

pub trait BitIndex {
    fn bit_index(self) -> u32;
}

macro_rules! impl_bit_index_unsigned {
	($($ty:ty),+ $(,)?) => {
		$(
			impl BitIndex for $ty {
				fn bit_index(self) -> u32 {
					self as u32
				}
			}
		)+
	};
}

impl_bit_index_unsigned!(u8, u16, u32, u64, usize);

#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct Bitset<T, Elem = u32> {
    set: T,
    marker: PhantomData<fn(Elem) -> Elem>,
}

impl<T: UnsignedInt, Elem> Default for Bitset<T, Elem> {
    fn default() -> Self {
        Self {
            set: T::default(),
            marker: PhantomData,
        }
    }
}

impl<T: UnsignedInt, Elem> Bitset<T, Elem> {
    pub const MAX_COUNT: u32 = T::BITS;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_elems<I>(elems: I) -> Self
    where
        I: IntoIterator<Item = Elem>,
        Elem: BitIndex,
    {
        elems.into_iter().collect()
    }

    pub fn from_rep(rep: T) -> Self {
        Self {
            set: rep,
            marker: PhantomData,
        }
    }

    pub fn rep(self) -> T {
        self.set
    }

    pub fn count(&self) -> u32 {
        bit_count(self.set)
    }

    pub fn clear(&mut self) {
        self.set = T::default();
    }
}

impl<T: UnsignedInt, Elem: BitIndex> Bitset<T, Elem> {
    pub fn contains(&self, elem: Elem) -> bool {
        test_bit(self.set, elem.bit_index())
    }

    pub fn add(&mut self, elem: Elem) -> bool {
        let index = elem.bit_index();
        if test_bit(self.set, index) {
            false
        } else {
            store_set_bit(&mut self.set, index);
            true
        }
    }

    pub fn remove(&mut self, elem: Elem) -> bool {
        let index = elem.bit_index();
        if test_bit(self.set, index) {
            store_clear_bit(&mut self.set, index);
            true
        } else {
            false
        }
    }

    pub fn remove_max(&mut self, max: Elem) {
        self.set = self.set & bit_max::<T>(max.bit_index());
    }
}

impl<T: UnsignedInt, Elem: BitIndex> FromIterator<Elem> for Bitset<T, Elem> {
    fn from_iter<I: IntoIterator<Item = Elem>>(iter: I) -> Self {
        let mut bitset = Self::new();
        for elem in iter {
            bitset.add(elem);
        }
        bitset
    }
}

impl<T: UnsignedInt, Elem: BitIndex, const N: usize> From<[Elem; N]> for Bitset<T, Elem> {
    fn from(value: [Elem; N]) -> Self {
        Self::from_elems(value)
    }
}

impl<T: Eq, Elem> PartialEq for Bitset<T, Elem> {
    fn eq(&self, other: &Self) -> bool {
        self.set == other.set
    }
}

impl<T: Eq, Elem> Eq for Bitset<T, Elem> {}

impl<T: Ord, Elem> PartialOrd for Bitset<T, Elem> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.set.cmp(&other.set))
    }
}

impl<T: Ord, Elem> Ord for Bitset<T, Elem> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.set.cmp(&other.set)
    }
}
