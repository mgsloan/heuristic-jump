//! The byte-shaped half of the vocabulary `design/rope-modifications.md` §2
//! puts in this crate: `shared` depends on `rope`, so the dependency cannot
//! run the other way and these cannot live in `shared`.
//!
//! Ours, not upstream's — this whole file is a patch, recorded in
//! `vendor/README.md`. Nothing in rope uses these types yet; converting the
//! signatures is §4's sweep and is a campaign of its own.

use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};

/// A position in a document. Never a UTF-16 offset, and never a quantity —
/// `ByteLen` is the quantity, and keeping them apart is what makes "advance a
/// position by a length" and "how far apart are these positions" different
/// signatures (`rope-modifications.md` §4).
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Offset(pub usize);

/// A quantity of bytes. Also what `resolution.md`'s `bytes_scanned` counts:
/// one byte quantity in the workspace, not two.
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ByteLen(pub usize);

#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ByteRange {
    pub start: Offset,
    pub end: Offset,
}

impl Offset {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(usize::MAX);
}

impl ByteLen {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(usize::MAX);
}

// There is deliberately no `From<ByteLen> for Offset`: turning a length
// into a position means measuring from somewhere, so it is spelled
// `Offset::ZERO + len`, which names the origin.

impl Add<ByteLen> for Offset {
    type Output = Self;

    fn add(self, other: ByteLen) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub<ByteLen> for Offset {
    type Output = Self;

    fn sub(self, other: ByteLen) -> Self {
        debug_assert!(other.0 <= self.0);
        Self(self.0 - other.0)
    }
}

/// The distance between two positions, which is a quantity and not a position.
impl Sub for Offset {
    type Output = ByteLen;

    fn sub(self, other: Self) -> ByteLen {
        debug_assert!(other <= self);
        ByteLen(self.0 - other.0)
    }
}

impl AddAssign<ByteLen> for Offset {
    fn add_assign(&mut self, other: ByteLen) {
        self.0 += other.0;
    }
}

impl SubAssign<ByteLen> for Offset {
    fn sub_assign(&mut self, other: ByteLen) {
        debug_assert!(other.0 <= self.0);
        self.0 -= other.0;
    }
}

impl Add for ByteLen {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for ByteLen {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        debug_assert!(other <= self);
        Self(self.0 - other.0)
    }
}

impl AddAssign for ByteLen {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl SubAssign for ByteLen {
    fn sub_assign(&mut self, other: Self) {
        debug_assert!(other <= *self);
        self.0 -= other.0;
    }
}

impl ByteRange {
    pub const EMPTY: Self = Self {
        start: Offset::ZERO,
        end: Offset::ZERO,
    };

    pub fn new(start: Offset, end: Offset) -> Self {
        Self { start, end }
    }

    pub fn len(self) -> ByteLen {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.end <= self.start
    }

    /// Half-open, as every range in this crate is: the end offset is not in
    /// the range.
    pub fn contains(self, at: Offset) -> bool {
        self.start <= at && at < self.end
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Empty when they do not overlap, rather than `None`: an empty range is
    /// already representable and every caller here goes on to iterate it.
    pub fn intersect(self, other: Self) -> Self {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        if end <= start {
            Self { start, end: start }
        } else {
            Self { start, end }
        }
    }
}

// Display prints the bare number, because `Point`'s own
// `write!(f, "Point({}:{})", self.row, self.column)` has to keep its output
// once its fields are newtypes (`rope-modifications.md` §4). It cannot be
// derived, so all seven are written out.

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for ByteLen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for ByteRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}
