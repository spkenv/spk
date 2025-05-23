// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
use std::future::Future;
use std::iter::FromIterator;
use std::sync::Arc;

use arc_swap::ArcSwap;
use itertools::Itertools;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::name::{OptName, OptNameBuf, PkgName};
use crate::spec_ops::EnvName;

mod error;
mod filters;
mod format;

pub use error::{Error, Result};
pub use filters::{OptFilter, get_host_options_filters};

#[cfg(test)]
#[path = "./option_map_test.rs"]
mod option_map_test;

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

/// A lazy, thread-safe, cache of the host options.
///
/// Use [`HostOptions::get`] to get an owned copy of the options.
pub struct HostOptions(Lazy<ArcSwap<Result<OptionMap>>>);

impl HostOptions {
    /// Return an owned copy of the host options.
    pub fn get(&self) -> Result<OptionMap> {
        (**self.0.load()).clone()
    }

    /// Detect and return the default options for the current host system.
    fn host_options() -> Result<OptionMap> {
        let mut opts = OptionMap::default();
        opts.insert(OptName::os().to_owned(), std::env::consts::OS.into());
        opts.insert(OptName::arch().to_owned(), std::env::consts::ARCH.into());

        let info = match sys_info::linux_os_release() {
            Ok(i) => i,
            Err(err) => {
                return Err(Error::String(format!("Failed to get linux info: {err:?}")));
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

    /// Change [`HOST_OPTIONS`] to return the provided substitute options for
    /// the duration of the given future.
    ///
    /// This method is intended to only be used by tests.
    ///
    /// There is no guarantee that some other concurrent task doesn't also
    /// change the options before the given future completes. It is recommended
    /// to use [`serial_test::serial`] and include the key "host_options" when
    /// using this function.
    pub async fn scoped_options<T>(
        &self,
        substitute_host_options: Result<OptionMap>,
        f: T,
    ) -> T::Output
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let current_options = HOST_OPTIONS.0.swap(Arc::new(substitute_host_options));
        let result = f.await;
        HOST_OPTIONS.0.store(current_options);
        result
    }
}

pub static HOST_OPTIONS: HostOptions = HostOptions(Lazy::new(|| {
    ArcSwap::new(Arc::new(HostOptions::host_options()))
}));

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
        let items: Vec<_> = self.iter().map(|(n, v)| format!("{n}: {v}")).collect();
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
            let var_name = format!("SPK_OPT_{}", name.env_name());
            out.insert(var_name, value.into());
        }
        out
    }

    /// Return only the options in this map that are not package-specific
    pub fn global_options(&self) -> Self {
        self.iter()
            .filter(|(k, _v)| !k.contains('.'))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
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

    /// Create a yaml mapping from this map, un-flattening all package options.
    ///
    /// OptionMaps hold package-specific options under dot-notated keys, eg `python.abi`.
    /// This function will split those options into sub-objects, creating a two-level
    /// mapping instead. In the case where there is a value for both `python` and `python.abi`
    /// the former will be dropped.
    pub fn to_yaml_value_expanded(&self) -> serde_yaml::Mapping {
        use serde_yaml::{Mapping, Value};
        let mut yaml = Mapping::default();
        for (key, value) in self.iter() {
            let target = match key.namespace() {
                Some(ns) => {
                    let ns = Value::String(ns.to_string());
                    let ns_value = yaml
                        .entry(ns)
                        .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));
                    if ns_value.as_mapping().is_none() {
                        *ns_value = serde_yaml::Value::Mapping(Default::default());
                    }
                    ns_value
                        .as_mapping_mut()
                        .expect("already validated that this is a mapping")
                }
                None => &mut yaml,
            };
            let key = serde_yaml::Value::String(key.base_name().to_string());
            let value = serde_yaml::Value::String(value.to_string());
            target.insert(key, value);
        }
        yaml
    }
}

impl<'de> Deserialize<'de> for OptionMap {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        pub struct OptionMapVisitor;

        impl<'de> serde::de::Visitor<'de> for OptionMapVisitor {
            type Value = OptionMap;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a mapping of option values")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut options = OptionMap::default();
                while let Some((name, value)) = map.next_entry::<OptNameBuf, Stringified>()? {
                    options.insert(name, value.0);
                }
                Ok(options)
            }
        }

        deserializer.deserialize_map(OptionMapVisitor)
    }
}

/// A type that deserializes a string from any scalar value
///
/// This allows non-string fields in yaml, such as `true`, to be
/// read-in as a string (eg: `"true"`) without getting an
/// unexpected or invalid type error
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Stringified(pub String);

impl std::ops::Deref for Stringified {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Stringified {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de> Deserialize<'de> for Stringified {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StringifyVisitor)
    }
}

#[derive(Default)]
pub struct StringifyVisitor;

impl serde::de::Visitor<'_> for StringifyVisitor {
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
    deserializer.deserialize_any(StringifyVisitor).map(|s| s.0)
}
