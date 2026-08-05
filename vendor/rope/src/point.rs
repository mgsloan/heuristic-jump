use std::{
    cmp::Ordering,
    fmt::{self, Debug},
    ops::{Add, AddAssign, Range, Sub},
};

/// A zero-indexed point in a text buffer consisting of a row and column.
#[derive(Clone, Copy, Default, Eq, PartialEq, Hash)]
pub struct Point {
    pub row: LineIndex,
    pub column: ByteColumn,
}

impl Debug for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Point({}:{})", self.row, self.column)
    }
}

impl Point {
    pub const MAX: Self = Self {
        row: LineIndex::MAX,
        column: ByteColumn::MAX,
    };

    pub fn new(row: LineIndex, column: ByteColumn) -> Self {
        Point { row, column }
    }

    pub fn row_range(range: Range<LineIndex>) -> Range<Self> {
        Point {
            row: range.start,
            column: ByteColumn::ZERO,
        }..Point {
            row: range.end,
            column: ByteColumn::ZERO,
        }
    }

    pub fn zero() -> Self {
        Point::new(LineIndex::ZERO, ByteColumn::ZERO)
    }

    pub fn parse_str(s: &str) -> Self {
        let mut point = Self::zero();
        for (row, line) in s.split('\n').enumerate() {
            point.row = LineIndex(row as u32);
            point.column = ByteColumn(line.len() as u32);
        }
        point
    }

    pub fn is_zero(&self) -> bool {
        self.row == LineIndex::ZERO && self.column == ByteColumn::ZERO
    }

    pub fn saturating_sub(self, other: Self) -> Self {
        if self < other {
            Self::zero()
        } else {
            self - other
        }
    }
}

impl<'a> Add<&'a Self> for Point {
    type Output = Point;

    fn add(self, other: &'a Self) -> Self::Output {
        self + *other
    }
}

impl Add for Point {
    type Output = Point;

    fn add(self, other: Self) -> Self::Output {
        if other.row == LineIndex::ZERO {
            Point::new(self.row, ByteColumn(self.column.0 + other.column.0))
        } else {
            Point::new(LineIndex(self.row.0 + other.row.0), other.column)
        }
    }
}

impl<'a> Sub<&'a Self> for Point {
    type Output = Point;

    fn sub(self, other: &'a Self) -> Self::Output {
        self - *other
    }
}

impl Sub for Point {
    type Output = Point;

    fn sub(self, other: Self) -> Self::Output {
        debug_assert!(other <= self);

        if self.row == other.row {
            Point::new(LineIndex::ZERO, ByteColumn(self.column.0 - other.column.0))
        } else {
            Point::new(LineIndex(self.row.0 - other.row.0), self.column)
        }
    }
}

impl<'a> AddAssign<&'a Self> for Point {
    fn add_assign(&mut self, other: &'a Self) {
        *self += *other;
    }
}

impl AddAssign<Self> for Point {
    fn add_assign(&mut self, other: Self) {
        if other.row == LineIndex::ZERO {
            self.column.0 += other.column.0;
        } else {
            self.row.0 += other.row.0;
            self.column = other.column;
        }
    }
}

impl PartialOrd for Point {
    fn partial_cmp(&self, other: &Point) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Point {
    #[cfg(target_pointer_width = "64")]
    fn cmp(&self, other: &Point) -> Ordering {
        let a = ((self.row.0 as usize) << 32) | self.column.0 as usize;
        let b = ((other.row.0 as usize) << 32) | other.column.0 as usize;
        a.cmp(&b)
    }

    #[cfg(target_pointer_width = "32")]
    fn cmp(&self, other: &Point) -> Ordering {
        match self.row.0.cmp(&other.row.0) {
            Ordering::Equal => self.column.cmp(&other.column),
            comparison @ _ => comparison,
        }
    }
}

// -- Ours, not upstream's ---------------------------------------------------
//
// `design/rope-modifications.md` §2 puts the line-shaped newtypes here, beside
// the type whose fields they are: §4's sweep is done, so `Point.row` above is
// a `LineIndex` and `Point.column` a `ByteColumn`.
//
// `CharCount` lands here rather than in `point_utf16.rs` because it is not a
// UTF-16 quantity: it counts Unicode scalar values, and it exists precisely
// because `ChunkSlice::first_line_chars` and `Point.column` are both "how far
// into a line" in different units.
//
// None of the four gets arithmetic. Adding two line numbers is meaningless and
// there is no length interpretation to rescue it, so rope unwraps explicitly
// where it needs the arithmetic and the unwrap is visible where it happens.

/// A zero-based line.
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LineIndex(pub u32);

/// `Point.column`: bytes into the line.
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ByteColumn(pub u32);

/// Unicode scalar values -- the fourth unit this crate measures in, after
/// bytes, UTF-16 code units and lines.
///
/// `usize` and not `u32`, which is the one place this family departs from the
/// line-shaped three beside it. `rope-modifications.md` §2: it is "the width
/// of the widest thing it has to hold rather than a preference" -- upstream's
/// `TextSummary.chars` accumulates across the whole rope and is a `usize`
/// there, so a `u32` here would cap a summary at 4G scalar values, which is an
/// edit to the arithmetic rather than to the representation and §3 forbids it.
/// The bound `Point.row` imposes on itself is not an argument for imposing
/// another one here.
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CharCount(pub usize);

impl LineIndex {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u32::MAX);
}

impl ByteColumn {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u32::MAX);
}

impl CharCount {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(usize::MAX);
}

impl fmt::Display for LineIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for ByteColumn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for CharCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
