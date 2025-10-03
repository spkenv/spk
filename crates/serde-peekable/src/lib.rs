// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk
//
// Originally sourced from:
// https://gist.github.com/giuseppe998e/0b4f7d92de772e081a90b8003c986110

//! Provides peeking into serde deserialization primitives, for use in [`serde::de::Deserialize`] implementations.

#![warn(missing_docs)]

use std::marker::PhantomData;

use serde::__private::de as private_de;
use serde::de;

/// Wraps around a Serde's MapAccess, providing the ability
/// to peek at the next key and/or value without consuming it.
pub struct PeekableMapAccess<'de, A> {
    map: A,
    peeked_key: Option<Option<private_de::Content<'de>>>,
    peeked_value: Option<private_de::Content<'de>>,
}

impl<'de, A> PeekableMapAccess<'de, A>
where
    A: de::MapAccess<'de>,
{
    /// Peeks at the next key in the map without consuming it.
    pub fn peek_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, A::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let key_ref = match self.peeked_key.as_ref() {
            Some(key_ref) => key_ref,
            None => {
                self.peeked_key = Some(self.map.next_key::<private_de::Content<'de>>()?);
                self.peeked_value = None; // Clears the previous peeked value

                // SAFETY: a `None` variant for `self` would have been replaced by a `Some`
                // variant in the code above.
                unsafe { self.peeked_key.as_ref().unwrap_unchecked() }
            }
        };

        match key_ref {
            Some(key_ref) => {
                let deserializer = private_de::ContentRefDeserializer::new(key_ref);
                seed.deserialize(deserializer).map(Some)
            }
            None => Ok(None),
        }
    }

    /// Peeks at the next value in the map without consuming it.
    fn peek_value_seed<V>(&mut self, seed: V) -> Result<V::Value, A::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value_ref = match self.peeked_value.as_ref() {
            Some(value_ref) => value_ref,
            None => {
                self.peeked_value = Some(self.map.next_value::<private_de::Content<'de>>()?);

                // SAFETY: a `None` variant for `self` would have been replaced by a `Some`
                // variant in the code above.
                unsafe { self.peeked_value.as_ref().unwrap_unchecked() }
            }
        };

        let deserializer = private_de::ContentRefDeserializer::new(value_ref);
        seed.deserialize(deserializer)
    }

    /// Peeks at the next key in the map without consuming it.
    ///
    /// This method exists as a convenience for `Deserialize` implementations.
    #[inline]
    pub fn peek_key<K>(&mut self) -> Result<Option<K>, A::Error>
    where
        K: de::Deserialize<'de>,
    {
        self.peek_key_seed(PhantomData)
    }

    /// Peeks at the next value in the map without consuming it.
    ///
    /// This method exists as a convenience for `Deserialize` implementations.
    #[inline]
    pub fn peek_value<V>(&mut self) -> Result<V, A::Error>
    where
        V: de::Deserialize<'de>,
    {
        self.peek_value_seed(PhantomData)
    }
}

impl<'de, A> de::MapAccess<'de> for PeekableMapAccess<'de, A>
where
    A: de::MapAccess<'de>,
{
    type Error = A::Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        match self.peeked_key.take() {
            Some(Some(key)) => {
                let deserializer = private_de::ContentDeserializer::new(key);
                seed.deserialize(deserializer).map(Some)
            }
            Some(None) => Ok(None),
            None => self.map.next_key_seed(seed),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        self.peeked_key = None; // Clears the previous peeked key

        match self.peeked_value.take() {
            Some(value) => {
                let deserializer = private_de::ContentDeserializer::new(value);
                seed.deserialize(deserializer)
            }
            None => self.map.next_value_seed(seed),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        self.map.size_hint()
    }
}

impl<'de, A> From<A> for PeekableMapAccess<'de, A>
where
    A: de::MapAccess<'de>,
{
    fn from(map: A) -> Self {
        Self {
            map,
            peeked_key: None,
            peeked_value: None,
        }
    }
}

/// Wraps around a Serde's SeqAccess, providing the ability
/// to peek at the next element without consuming it.
pub struct PeekableSeqAccess<'de, S> {
    seq: S,
    peeked: Option<Option<private_de::Content<'de>>>,
}

impl<'de, S> PeekableSeqAccess<'de, S>
where
    S: de::SeqAccess<'de>,
{
    /// Peeks at the next element in the sequence without consuming it.
    pub fn peek_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, S::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let elem_ref = match self.peeked.as_ref() {
            Some(elem_ref) => elem_ref,
            None => {
                self.peeked = Some(self.seq.next_element::<private_de::Content<'de>>()?);

                // SAFETY: a `None` variant for `self` would have been replaced by a `Some`
                // variant in the code above.
                unsafe { self.peeked.as_ref().unwrap_unchecked() }
            }
        };

        match elem_ref {
            Some(elem_ref) => {
                let deserializer = private_de::ContentRefDeserializer::new(elem_ref);
                seed.deserialize(deserializer).map(Some)
            }
            None => Ok(None),
        }
    }

    /// Peeks at the next element in the sequence without consuming it.
    ///
    /// This method exists as a convenience for `Deserialize` implementations.
    #[inline]
    pub fn peek_element<T>(&mut self) -> Result<Option<T>, S::Error>
    where
        T: de::Deserialize<'de>,
    {
        self.peek_element_seed(PhantomData)
    }
}

impl<'de, S> de::SeqAccess<'de> for PeekableSeqAccess<'de, S>
where
    S: de::SeqAccess<'de>,
{
    type Error = S::Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        match self.peeked.take() {
            None => self.seq.next_element_seed(seed),
            Some(Some(elem)) => {
                let deserializer = private_de::ContentDeserializer::new(elem);
                seed.deserialize(deserializer).map(Some)
            }
            Some(None) => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        self.seq.size_hint()
    }
}

impl<'de, S> From<S> for PeekableSeqAccess<'de, S>
where
    S: de::SeqAccess<'de>,
{
    fn from(seq: S) -> Self {
        Self { seq, peeked: None }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use serde::de::{MapAccess, Visitor};
    use serde::{Deserialize, Deserializer};

    #[test]
    fn test_peek_and_consume() {
        struct Test;

        impl<'de> Deserialize<'de> for Test {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct TestVisitor;

                impl<'de> Visitor<'de> for TestVisitor {
                    type Value = Test;

                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str("a map")
                    }

                    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
                    where
                        A: MapAccess<'de>,
                    {
                        let mut map = super::PeekableMapAccess::from(map);

                        // 1. Peek at the first key
                        let key: Option<String> = map.peek_key()?;
                        assert_eq!(key.as_deref(), Some("a"));

                        // 2. Consume the first key-value pair
                        let (key, value): (String, i32) = map.next_entry()?.unwrap();
                        assert_eq!(key, "a");
                        assert_eq!(value, 1);

                        // 3. Peek at the second key
                        let key: Option<String> = map.peek_key()?;
                        assert_eq!(key.as_deref(), Some("b"));

                        // 4. Peek at the second value
                        let value: i32 = map.peek_value()?;
                        assert_eq!(value, 2);

                        // 5. Peek at the second key again
                        let key: Option<String> = map.peek_key()?;
                        assert_eq!(key.as_deref(), Some("b"));

                        // 6. Consume the second key-value pair
                        let (key, value): (String, i32) = map.next_entry()?.unwrap();
                        assert_eq!(key, "b");
                        assert_eq!(value, 2);

                        // 7. Ensure map is empty
                        assert!(map.next_key::<String>()?.is_none());

                        Ok(Test)
                    }
                }

                deserializer.deserialize_map(TestVisitor)
            }
        }

        let json = r#"{"a": 1, "b": 2}"#;
        let _: Test = serde_json::from_str(json).unwrap();
    }

    #[test]
    fn test_peek_value_first() {
        struct Test;

        impl<'de> Deserialize<'de> for Test {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct TestVisitor;

                impl<'de> Visitor<'de> for TestVisitor {
                    type Value = Test;

                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str("a map")
                    }

                    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
                    where
                        A: MapAccess<'de>,
                    {
                        let mut map = super::PeekableMapAccess::from(map);

                        // 1. Consume the first key
                        let key: String = map.next_key()?.unwrap();
                        assert_eq!(key, "a");

                        // 2. Peek at the first value
                        let value: i32 = map.peek_value()?;
                        assert_eq!(value, 1);

                        // 3. Peek at the first value again
                        let value: i32 = map.peek_value()?;
                        assert_eq!(value, 1);

                        // 4. Consume the first value
                        let value: i32 = map.next_value()?;
                        assert_eq!(value, 1);

                        // 5. Peek at the second key
                        let key: Option<String> = map.peek_key()?;
                        assert_eq!(key.as_deref(), Some("b"));

                        // 6. Consume the second key-value pair
                        let (key, value): (String, i32) = map.next_entry()?.unwrap();
                        assert_eq!(key, "b");
                        assert_eq!(value, 2);

                        Ok(Test)
                    }
                }

                deserializer.deserialize_map(TestVisitor)
            }
        }

        let json = r#"{"a": 1, "b": 2}"#;
        let _: Test = serde_json::from_str(json).unwrap();
    }
}