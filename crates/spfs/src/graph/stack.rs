// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use encoding::Digest;

use crate::{encoding, Error, Result};

#[cfg(test)]
#[path = "./stack_test.rs"]
mod stack_test;

/// A stack is an ordered set of layers with the conceptual top
/// layer overriding and shadowing entries in the lower ones.
// A crude linked list is used to make pushing more efficient since
// we remove duplicates during this process and there is rarely
// a need to lookup items by index, only iterate through them all
#[derive(Default, Clone, Eq, PartialEq)]
pub struct Stack {
    /// The bottom of the layer stack
    bottom: Option<Box<Entry>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct Entry {
    value: Digest,
    next: Option<Box<Entry>>,
}

impl Entry {
    fn new(value: Digest) -> Box<Self> {
        Box::new(Self { value, next: None })
    }
}

impl std::fmt::Debug for Stack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter_bottom_up()).finish()
    }
}

impl Stack {
    pub fn from_digestible<E, I>(items: I) -> Result<Self>
    where
        E: encoding::Digestible,
        Error: std::convert::From<E::Error>,
        I: IntoIterator<Item = E>,
    {
        let mut stack = Self { bottom: None };
        for item in items.into_iter() {
            stack.push(item.digest()?);
        }
        Ok(stack)
    }

    pub fn is_empty(&self) -> bool {
        self.bottom.is_none()
    }

    pub fn clear(&mut self) {
        self.bottom.take();
    }

    /// Add an item to the top of the stack.
    ///
    /// If the digest already exists in this stack, the previous
    /// one is removed from it's position.
    ///
    /// False is returned if no change was made to the stack
    /// because the digest was already at the top.
    pub fn push(&mut self, digest: Digest) -> bool {
        let mut node = &mut self.bottom;

        // remove any node that contains the same digest value
        // as we walk to the end of the stack
        loop {
            match node {
                n @ None => {
                    let _ = n.insert(Entry::new(digest));
                    break;
                }
                Some(entry) if entry.value == digest => {
                    // if this is already the last node, then
                    // report that no change is needed
                    if entry.next.is_none() {
                        return false;
                    }
                    // replace this node with it's next entry,
                    // removing it from the stack
                    let replace = entry.next.take();
                    *node = replace;
                }
                Some(entry) => {
                    node = &mut entry.next;
                }
            };
        }
        true
    }

    /// Iterate the stack lazily from bottom to top
    pub fn iter_bottom_up(&self) -> Iter<'_> {
        Iter(self.bottom.as_deref())
    }

    /// Return the digests in this stack in
    /// reverse (top-down) order.
    ///
    /// This must traverse the entire stack and perform a reversal.
    /// Prefer [`Self::iter_bottom_up`] whenever possible.
    pub fn to_top_down(&self) -> Vec<Digest> {
        let mut entries = self.iter_bottom_up().collect::<Vec<_>>();
        entries.reverse();
        entries
    }
}

impl From<Digest> for Stack {
    fn from(value: Digest) -> Self {
        Self {
            bottom: Some(Entry::new(value)),
        }
    }
}

impl FromIterator<Digest> for Stack {
    fn from_iter<T: IntoIterator<Item = Digest>>(iter: T) -> Self {
        let mut stack = Self::default();
        stack.extend(iter);
        stack
    }
}

impl Extend<Digest> for Stack {
    fn extend<T: IntoIterator<Item = Digest>>(&mut self, iter: T) {
        for item in iter.into_iter() {
            self.push(item);
        }
    }
}

impl<'a> Extend<&'a Digest> for Stack {
    fn extend<T: IntoIterator<Item = &'a Digest>>(&mut self, iter: T) {
        self.extend(iter.into_iter().copied())
    }
}

impl<'de> serde::Deserialize<'de> for Stack {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = Vec::<Digest>::deserialize(deserializer)?;
        // for historical reasons, and to remain backward-compatible,
        // stacks are stored in reverse (top-down) order
        Ok(Self::from_iter(s.into_iter().rev()))
    }
}

impl serde::Serialize for Stack {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // for historical reasons, and to remain backward-compatible, platform
        // stacks are stored in reverse (top-down) order. Unfortunately, this makes
        // serialization a little costly for large stacks
        serializer.collect_seq(self.to_top_down())
    }
}

pub struct Iter<'a>(Option<&'a Entry>);

impl<'a> Iterator for Iter<'a> {
    type Item = Digest;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0.take() {
            Some(current) => {
                self.0 = current.next.as_deref();
                Some(current.value)
            }
            None => None,
        }
    }
}
