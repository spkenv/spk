// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{BTreeMap, HashMap};
use std::convert::{TryFrom, TryInto};
use std::iter::FromIterator;
use std::sync::Arc;

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::name::{OptName, OptNameBuf, PkgName};

mod error;
mod format;

pub use error::{Error, Result};

#[cfg(test)]
#[path = "./option_map_test.rs"]
mod option_map_test;

// given option digests are namespaced by the package itself,
// there is a slim likelihood of collision, so we roll the dice
// also must be a multiple of 8 to be decodable which is generally
// a nice way to handle validation / and 16 is a lot
pub const DIGEST_SIZE: usize = 8;

type Digest = [char; DIGEST_SIZE];

/// Create a set of options from a simple mapping.
///
/// ```
/// # #[macro_use] extern crate spk_schema_foundation;
/// # fn main() {
/// option_map!{
///   "debug" => "on",
///   "python.abi" => "cp37m"
/// };
/// # }
/// ```
#[macro_export]
macro_rules! option_map {
    ($($k:expr => $v:expr),* $(,)?) => {{
        #[allow(unused_imports)]
        use {
            std::convert::TryFrom,
            $crate::name::OptNameBuf,
            $crate::option_map::OptionMap
        };
        #[allow(unused_mut)]
        let mut opts = OptionMap::default();
        $(opts.insert(
            OptNameBuf::try_from($k).expect("invalid option name"),
            $v.into()
        );)*
        opts
    }};
}

/// Detect and return the default options for the current host system.
pub fn host_options() -> Result<OptionMap> {
    let mut opts = OptionMap::default();
    opts.insert(OptName::os().to_owned(), std::env::consts::OS.into());
    opts.insert(OptName::arch().to_owned(), std::env::consts::ARCH.into());

    let info = match sys_info::linux_os_release() {
        Ok(i) => i,
        Err(err) => {
            return Err(Error::String(format!(
                "Failed to get linux info: {:?}",
                err
            )))
        }
    };

    if let Some(id) = info.id {
        opts.insert(OptName::distro().to_owned(), id.clone());
        match OptNameBuf::try_from(id) {
            Ok(id) => {
                if let Some(version_id) = info.version_id {
                    opts.insert(id, version_id);
                }
            }
            Err(err) => {
                tracing::warn!("Reported distro id is not a valid option name: {err}");
            }
        }
    }

    Ok(opts)
}

/// A set of values for package build options.
#[derive(Default, Clone, Hash, PartialEq, Eq, Serialize, Ord, PartialOrd)]
#[serde(transparent)]
pub struct OptionMap {
    options: BTreeMap<OptNameBuf, String>,
}

impl std::ops::Deref for OptionMap {
    type Target = BTreeMap<OptNameBuf, String>;

    fn deref(&self) -> &Self::Target {
        &self.options
    }
}

impl std::ops::DerefMut for OptionMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.options
    }
}

impl From<&Arc<BTreeMap<OptNameBuf, String>>> for OptionMap {
    fn from(hm: &Arc<BTreeMap<OptNameBuf, String>>) -> Self {
        Self {
            options: (**hm).clone(),
        }
    }
}

impl FromIterator<(OptNameBuf, String)> for OptionMap {
    fn from_iter<T: IntoIterator<Item = (OptNameBuf, String)>>(iter: T) -> Self {
        Self {
            options: BTreeMap::from_iter(iter),
        }
    }
}

impl std::fmt::Debug for OptionMap {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for OptionMap {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let items: Vec<_> = self.iter().map(|(n, v)| format!("{}: {}", n, v)).collect();
        f.write_fmt(format_args!("{{{}}}", items.join(", ")))
    }
}

impl IntoIterator for OptionMap {
    type IntoIter = std::collections::btree_map::IntoIter<OptNameBuf, String>;
    type Item = (OptNameBuf, String);

    fn into_iter(self) -> Self::IntoIter {
        self.options.into_iter()
    }
}

impl OptionMap {
    /// Return the data of these options as environment variables.
    pub fn to_environment(&self) -> HashMap<String, String> {
        let mut out = HashMap::default();
        for (name, value) in self.iter() {
            let var_name = format!("SPK_OPT_{}", name);
            out.insert(var_name, value.into());
        }
        out
    }

    fn items(&self) -> Vec<(OptNameBuf, String)> {
        self.options
            .iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }

    pub fn digest(&self) -> Digest {
        let mut hasher = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
        for (name, value) in self.items() {
            hasher.update(name.as_bytes());
            hasher.update(b"=");
            hasher.update(value.as_bytes());
            hasher.update(&[0]);
        }

        let digest = hasher.finish();
        let encoded = data_encoding::BASE32.encode(digest.as_ref());
        encoded
            .chars()
            .take(DIGEST_SIZE)
            .collect_vec()
            .try_into()
            .unwrap() // sha1 digests are always greater than 8 characters
    }

    /// The digest of this option map as a proper length string
    pub fn digest_str(&self) -> String {
        self.digest().iter().collect()
    }

    /// Return only the options in this map that are not package-specific
    pub fn global_options(&self) -> Self {
        self.iter()
            .filter_map(|(k, v)| (!k.contains('.')).then(|| (k.to_owned(), v.to_owned())))
            .collect()
    }

    /// Return the set of options given for the specific named package.
    pub fn package_options_without_global<S: AsRef<PkgName>>(&self, name: S) -> Self {
        let pkg = name.as_ref();
        let mut options = OptionMap::default();
        for (name, value) in self.iter() {
            if name.namespace() == Some(pkg) {
                options.insert(name.without_namespace().to_owned(), value.clone());
            }
        }
        options
    }

    /// Return the set of options relevant to the named package.
    pub fn package_options<S: AsRef<PkgName>>(&self, name: S) -> Self {
        let mut options = self.global_options();
        options.append(&mut self.package_options_without_global(name));
        options
    }

    /// Remove option-related values from the given environment variables
    pub fn clean_environment(env: &mut HashMap<String, String>) {
        let to_remove = env
            .keys()
            .filter(|name| name.starts_with("SPK_OPT_"))
            .map(|k| k.to_owned())
            .collect_vec();
        for name in to_remove.into_iter() {
            env.remove(&name);
        }
    }
}

impl<'de> Deserialize<'de> for OptionMap {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Default)]
        pub struct OptionMapVisitor {
            inner: OptionMap,
        }

        impl<'de> serde::de::Visitor<'de> for OptionMapVisitor {
            type Value = OptionMap;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a mapping of option values")
            }

            fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                while let Some((name, value)) = map.next_entry::<OptNameBuf, Stringified>()? {
                    self.inner.insert(name, value.0);
                }
                Ok(self.inner)
            }
        }

        deserializer.deserialize_map(OptionMapVisitor::default())
    }
}

/// A type that deserializes a string from any scalar value
///
/// This allows non-string fields in yaml, such as `true`, to be
/// read-in as a string (eg: `"true"`) without getting an
/// unexpected or invalid type error
pub struct Stringified(pub String);

impl std::ops::Deref for Stringified {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Stringified {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StringifyVisitor::default())
    }
}

#[derive(Default)]
pub struct StringifyVisitor;

impl<'de> serde::de::Visitor<'de> for StringifyVisitor {
    type Value = Stringified;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a scalar value")
    }

    fn visit_bool<E>(self, v: bool) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Stringified(v.to_string()))
    }

    fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Stringified(v.to_string()))
    }

    fn visit_i128<E>(self, v: i128) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Stringified(v.to_string()))
    }

    fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Stringified(v.to_string()))
    }

    fn visit_u128<E>(self, v: u128) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Stringified(v.to_string()))
    }

    fn visit_f64<E>(self, v: f64) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Stringified(v.to_string()))
    }

    fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Stringified(v.to_string()))
    }

    fn visit_string<E>(self, v: String) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Stringified(v))
    }

    fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Stringified(String::new()))
    }
}

/// Deserialize any reasonable scalar option (int, float, str) to a string value
pub fn string_from_scalar<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer
        .deserialize_any(StringifyVisitor::default())
        .map(|s| s.0)
}
