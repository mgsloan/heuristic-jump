use std::{
    cmp::Ordering,
    ops::{Add, AddAssign, Sub},
};

use crate::point::LineIndex;

#[derive(Clone, Copy, Default, Eq, PartialEq, Debug, Hash)]
pub struct PointUtf16 {
    pub row: LineIndex,
    pub column: Utf16Column,
}

impl PointUtf16 {
    pub const MAX: Self = Self {
        row: LineIndex::MAX,
        column: Utf16Column::MAX,
    };

    pub fn new(row: LineIndex, column: Utf16Column) -> Self {
        PointUtf16 { row, column }
    }

    pub fn zero() -> Self {
        PointUtf16::new(LineIndex::ZERO, Utf16Column::ZERO)
    }

    pub fn is_zero(&self) -> bool {
        self.row == LineIndex::ZERO && self.column == Utf16Column::ZERO
    }

    pub fn saturating_sub(self, other: Self) -> Self {
        if self < other {
            Self::zero()
        } else {
            self - other
        }
    }
}

impl<'a> Add<&'a Self> for PointUtf16 {
    type Output = PointUtf16;

    fn add(self, other: &'a Self) -> Self::Output {
        self + *other
    }
}

impl Add for PointUtf16 {
    type Output = PointUtf16;

    fn add(self, other: Self) -> Self::Output {
        if other.row == LineIndex::ZERO {
            PointUtf16::new(self.row, Utf16Column(self.column.0 + other.column.0))
        } else {
            PointUtf16::new(LineIndex(self.row.0 + other.row.0), other.column)
        }
    }
}

impl<'a> Sub<&'a Self> for PointUtf16 {
    type Output = PointUtf16;

    fn sub(self, other: &'a Self) -> Self::Output {
        self - *other
    }
}

impl Sub for PointUtf16 {
    type Output = PointUtf16;

    fn sub(self, other: Self) -> Self::Output {
        debug_assert!(other <= self);

        if self.row == other.row {
            PointUtf16::new(
                LineIndex::ZERO,
                Utf16Column(self.column.0 - other.column.0),
            )
        } else {
            PointUtf16::new(LineIndex(self.row.0 - other.row.0), self.column)
        }
    }
}

impl<'a> AddAssign<&'a Self> for PointUtf16 {
    fn add_assign(&mut self, other: &'a Self) {
        *self += *other;
    }
}

impl AddAssign<Self> for PointUtf16 {
    fn add_assign(&mut self, other: Self) {
        if other.row == LineIndex::ZERO {
            self.column.0 += other.column.0;
        } else {
            self.row.0 += other.row.0;
            self.column = other.column;
        }
    }
}

impl PartialOrd for PointUtf16 {
    fn partial_cmp(&self, other: &PointUtf16) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PointUtf16 {
    #[cfg(target_pointer_width = "64")]
    fn cmp(&self, other: &PointUtf16) -> Ordering {
        let a = ((self.row.0 as usize) << 32) | self.column.0 as usize;
        let b = ((other.row.0 as usize) << 32) | other.column.0 as usize;
        a.cmp(&b)
    }

    #[cfg(target_pointer_width = "32")]
    fn cmp(&self, other: &PointUtf16) -> Ordering {
        match self.row.0.cmp(&other.row.0) {
            Ordering::Equal => self.column.cmp(&other.column),
            comparison @ _ => comparison,
        }
    }
}

// -- Ours, not upstream's ---------------------------------------------------
//
// `design/rope-modifications.md` §2. Distinct from `ByteColumn` in `point.rs`,
// and that distinction is most of the value of this change: the two are
// currently interchangeable `u32`s and must not be.

/// `PointUtf16.column`: UTF-16 code units into the line.
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Utf16Column(pub u32);

impl Utf16Column {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u32::MAX);
}

impl std::fmt::Display for Utf16Column {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
