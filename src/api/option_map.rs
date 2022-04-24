// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{BTreeMap, HashMap};
use std::convert::TryInto;
use std::iter::FromIterator;

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use sys_info;

#[cfg(test)]
#[path = "./option_map_test.rs"]
mod option_map_test;

// given option digests are namespaced by the package itself,
// there are slim likelyhoods of collision, so we roll the dice
// also must be a multiple of 8 to be decodable wich is generally
// a nice way to handle validation / and 16 is a lot
pub const DIGEST_SIZE: usize = 8;

type Digest = [char; DIGEST_SIZE];

/// Create a set of options from a simple mapping.
///
/// ```
/// # #[macro_use] extern crate spk;
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
        use $crate::api::OptionMap;
        #[allow(unused_mut)]
        let mut opts = OptionMap::default();
        $(opts.insert($k.into(), $v.into());)*
        opts
    }};
}

/// Detect and return the default options for the current host system.
pub fn host_options() -> crate::Result<OptionMap> {
    let mut opts = OptionMap::default();
    opts.insert("os".into(), std::env::consts::OS.into());
    opts.insert("arch".into(), std::env::consts::ARCH.into());

    let info = match sys_info::linux_os_release() {
        Ok(i) => i,
        Err(err) => {
            return Err(crate::Error::String(format!(
                "Failed to get linux info: {:?}",
                err
            )))
        }
    };

    if let Some(id) = info.id {
        opts.insert("distro".into(), id.clone());
        if let Some(version_id) = info.version_id {
            opts.insert(id, version_id);
        }
    }

    Ok(opts)
}

/// A set of values for package build options.
#[derive(Default, Clone, Hash, PartialEq, Eq, Serialize, Ord, PartialOrd)]
#[serde(transparent)]
pub struct OptionMap {
    options: BTreeMap<String, String>,
}

impl std::ops::Deref for OptionMap {
    type Target = BTreeMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.options
    }
}

impl std::ops::DerefMut for OptionMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.options
    }
}

impl FromIterator<(String, String)> for OptionMap {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
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
    type IntoIter = std::collections::btree_map::IntoIter<String, String>;
    type Item = (String, String);

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

    fn items(&self) -> Vec<(String, String)> {
        self.options
            .iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }
}

impl OptionMap {
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
    pub fn package_options_without_global<S: AsRef<str>>(&self, name: S) -> Self {
        let prefix = format!("{}.", name.as_ref());
        let mut options = OptionMap::default();
        for (key, value) in self.iter() {
            if let Some(key) = key.strip_prefix(prefix.as_str()) {
                options.insert(key.to_string(), value.to_string());
            }
        }
        options
    }

    /// Return the set of options relevant to the named package.
    pub fn package_options<S: AsRef<str>>(&self, name: S) -> Self {
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
        use serde_yaml::Value;
        let value = Value::deserialize(deserializer)?;
        let mapping = match value {
            Value::Mapping(m) => m,
            _ => {
                return Err(serde::de::Error::custom(
                    "expected yaml mapping for OptionMap",
                ))
            }
        };
        let mut options = OptionMap::default();
        for (name, value) in mapping.into_iter() {
            let name = String::deserialize(name)
                .map_err(|err| serde::de::Error::custom(err.to_string()))?;
            let value = string_from_scalar(value)
                .map_err(|err| serde::de::Error::custom(err.to_string()))?;
            options.options.insert(name, value);
        }
        Ok(options)
    }
}

/// Deserialize any reasonable scalar option (int, float, str) to a string value
pub(crate) fn string_from_scalar<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_yaml::Value;
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::String(s) => Ok(s),
        _ => Err(serde::de::Error::custom("expected scalar value")),
    }
}
