// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::version::Version;

/// A range of valid versions that has some kind of bound.
#[derive(Debug)]
pub(crate) enum LimitedValidRange<'a> {
    /// A range that is greater or equal to a version.
    RangeFrom(&'a Version),
    /// A range that is less than a version (exclusive bound).
    RangeTo(&'a Version),
    /// A range that is greater or equal to a starting version
    /// and less than an ending version (exclusive bound).
    Range(&'a Version, &'a Version),
}

impl<'a> LimitedValidRange<'a> {
    /// Return true if there exists any version that is valid in both ranges.
    fn intersects(&self, other: &ValidRange) -> bool {
        match other {
            ValidRange::Total => true,
            ValidRange::Range(r) => self.intersects_nonempty(r),
            ValidRange::Pair(p1, p2) => {
                self.intersects_nonempty(p1) || self.intersects_nonempty(p2)
            }
        }
    }

    /// Return true if there exists any version that is valid in both ranges.
    fn intersects_nonempty(&self, other: &Self) -> bool {
        // These comparisons are all implemented with a on the lhs
        match (self, other) {
            (LimitedValidRange::RangeFrom(_), LimitedValidRange::RangeFrom(_)) => {
                // a.. && b..
                true
            }
            (LimitedValidRange::RangeFrom(a), LimitedValidRange::RangeTo(b)) => {
                // a.. && ..b
                a < b
            }
            (LimitedValidRange::RangeFrom(a), LimitedValidRange::Range(_, b2)) => {
                // a.. && b1..b2
                a < b2
            }
            (LimitedValidRange::RangeTo(a), LimitedValidRange::RangeFrom(b)) => {
                // ..a && b..
                a > b
            }
            (LimitedValidRange::RangeTo(_), LimitedValidRange::RangeTo(_)) => {
                // ..a && ..b
                true
            }
            (LimitedValidRange::RangeTo(a), LimitedValidRange::Range(b1, _)) => {
                // ..a && b1..b2
                a > b1
            }
            (LimitedValidRange::Range(_, a2), LimitedValidRange::RangeFrom(b)) => {
                // a1..a2 && b..
                a2 > b
            }
            (LimitedValidRange::Range(a1, _), LimitedValidRange::RangeTo(b)) => {
                // a1..a2 && ..b
                a1 < b
            }
            (LimitedValidRange::Range(a1, a2), LimitedValidRange::Range(b1, b2)) => {
                // a1..a2 && b1..b2
                (a1 >= b1 && a1 < b2) || (a1 <= b1 && a2 > b1)
            }
        }
    }
}

impl<'a> std::fmt::Display for LimitedValidRange<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format these ranges using rust slice syntax.
        match self {
            LimitedValidRange::RangeFrom(a) => {
                write!(f, "{a}..")
            }
            LimitedValidRange::RangeTo(a) => {
                write!(f, "..{a}")
            }
            LimitedValidRange::Range(a, b) => {
                write!(f, "{a}..{b}")
            }
        }
    }
}

/// A range of valid versions.
#[derive(Debug)]
pub(crate) enum ValidRange<'a> {
    /// All version are valid.
    Total,
    /// Some versions are valid.
    Range(LimitedValidRange<'a>),
    /// A pair of valid version ranges, which may or may not overlap.
    Pair(LimitedValidRange<'a>, LimitedValidRange<'a>),
}

impl<'a> ValidRange<'a> {
    /// Return true if there exists any version that is valid in both ranges.
    pub(crate) fn intersects(&self, other: &ValidRange) -> bool {
        match self {
            ValidRange::Total => true,
            ValidRange::Range(r) => r.intersects(other),
            ValidRange::Pair(r1, r2) => r1.intersects(other) || r2.intersects(other),
        }
    }
}

impl<'a> std::fmt::Display for ValidRange<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidRange::Total => f.pad(".."),
            ValidRange::Range(r) => r.fmt(f),
            ValidRange::Pair(p1, p2) => {
                write!(f, "{p1},{p2}")
            }
        }
    }
}

pub(crate) trait CombineWith<Rhs> {
    /// Update a `ValidRange` from a range.
    ///
    /// A `ValidRange` starts off as totally valid.
    ///
    /// Combining with an additional range has the effect of
    /// invalidating the versions _not_ covered by the range,
    /// or covered by a previously combined range.
    ///
    /// ```ignore
    ///                                 0123456789
    /// Step 0, all versions are valid: YYYYYYYYYY
    /// Step 1, "combine" with 5..    : NNNNNYYYYY
    /// Step 2, "combine" with ..7    : NNNNNYYNNN
    ///
    ///                                 0123456789
    /// Step 0, all versions are valid: YYYYYYYYYY
    /// Step 1, "combine" with 3..    : NNNYYYYYYY
    /// Step 2, "combine" with 5..    : NNNNNYYYYY
    ///
    ///                                 0123456789
    /// Step 0, all versions are valid: YYYYYYYYYY
    /// Step 1, "combine" with ..3    : YYYNNNNNNN
    /// Step 2, "combine" with 5..    : YYYNNNYYYY
    /// ```
    fn restrict(&mut self, rhs: Rhs);
}

impl<'a, 'b> CombineWith<std::ops::RangeFrom<&'a Version>> for ValidRange<'b>
where
    'a: 'b,
{
    fn restrict(&mut self, rhs: std::ops::RangeFrom<&'a Version>) {
        *self = match self {
            ValidRange::Total => ValidRange::Range(LimitedValidRange::RangeFrom(rhs.start)),
            ValidRange::Range(LimitedValidRange::RangeFrom(lhs)) => {
                // "1.0.." && "2.0.." == "1.0.."
                ValidRange::Range(LimitedValidRange::RangeFrom((*lhs).min(rhs.start)))
            }
            ValidRange::Range(LimitedValidRange::RangeTo(lhs)) => {
                if *lhs < rhs.start {
                    // "..1.0" && "2.0.." == "..1.0,2.0.."
                    ValidRange::Pair(
                        LimitedValidRange::RangeTo(lhs),
                        LimitedValidRange::RangeFrom(rhs.start),
                    )
                } else {
                    // "..2.0" && "1.0.." == "1.0..2.0"
                    ValidRange::Range(LimitedValidRange::Range(rhs.start, lhs))
                }
            }
            ValidRange::Range(LimitedValidRange::Range(..)) => {
                // It is not possible to have a Range on the lhs because
                // the algorithm doesn't call `restrict` more than
                // twice on a given value.
                unreachable!();
            }
            ValidRange::Pair(..) => {
                // It is not possible to have a Pair on the lhs because
                // the algorithm doesn't call `restrict` more than
                // twice on a given value.
                unreachable!()
            }
        }
    }
}

impl<'a, 'b> CombineWith<std::ops::RangeTo<&'a Version>> for ValidRange<'b>
where
    'a: 'b,
{
    fn restrict(&mut self, rhs: std::ops::RangeTo<&'a Version>) {
        *self = match self {
            ValidRange::Total => ValidRange::Range(LimitedValidRange::RangeTo(rhs.end)),
            ValidRange::Range(LimitedValidRange::RangeFrom(lhs)) => {
                if *lhs < rhs.end {
                    // "1.0.." && "..2.0" == "1.0..2.0"
                    ValidRange::Range(LimitedValidRange::Range(lhs, rhs.end))
                } else {
                    // "2.0.." && "..1.0" == "..1.0,2.0.."
                    ValidRange::Pair(
                        LimitedValidRange::RangeTo(rhs.end),
                        LimitedValidRange::RangeFrom(lhs),
                    )
                }
            }
            ValidRange::Range(LimitedValidRange::RangeTo(lhs)) => {
                // "..1.0" && "..2.0" == "..2.0"
                ValidRange::Range(LimitedValidRange::RangeTo((*lhs).max(rhs.end)))
            }
            ValidRange::Range(LimitedValidRange::Range(..)) => {
                // It is not possible to have a Range on the lhs because
                // the algorithm doesn't call `restrict` more than
                // twice on a given value.
                unreachable!();
            }
            ValidRange::Pair(..) => {
                // It is not possible to have a Pair on the lhs because
                // the algorithm doesn't call `restrict` more than
                // twice on a given value.
                unreachable!()
            }
        }
    }
}
