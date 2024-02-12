// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

pub(super) struct NormalizedPartsIter<'a> {
    inner: std::slice::Iter<'a, u32>,
    skipped_zeros: usize,
    next_non_zero: Option<u32>,
    empty: bool,
}

impl NormalizedPartsIter<'_> {
    pub fn new(parts: &[u32]) -> NormalizedPartsIter<'_> {
        NormalizedPartsIter {
            inner: parts.iter(),
            skipped_zeros: 0,
            next_non_zero: None,
            empty: true,
        }
    }
}

impl<'a> Iterator for NormalizedPartsIter<'a> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(non_zero) = self.next_non_zero.take() {
                self.empty = false;
                if self.skipped_zeros > 0 {
                    self.skipped_zeros -= 1;
                    self.next_non_zero = Some(non_zero);
                    return Some(0);
                }
                return Some(non_zero);
            }
            let Some(next) = self.inner.next() else {
                if self.empty {
                    self.empty = false;
                    return Some(0);
                }
                return None;
            };
            if *next == 0 {
                self.skipped_zeros += 1;
            } else {
                self.next_non_zero = Some(*next);
            }
        }
    }
}

pub(super) struct MinimumPartsPartIter<'a> {
    parts: &'a [u32],
    minimum_parts: usize,
    pos: usize,
}

impl MinimumPartsPartIter<'_> {
    pub(super) fn new(parts: &[u32], minimum_parts: usize) -> MinimumPartsPartIter<'_> {
        MinimumPartsPartIter {
            parts,
            minimum_parts,
            pos: 0,
        }
    }
}

impl<'a> Iterator for MinimumPartsPartIter<'a> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos += 1;
        if !self.parts.is_empty() {
            let part = self.parts[0];
            self.parts = &self.parts[1..];
            return Some(part);
        }
        if self.pos - 1 < self.minimum_parts {
            return Some(0);
        }
        None
    }
}
