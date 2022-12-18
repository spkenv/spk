// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::fmt::Write;

use spk_schema_foundation::format::FormatOptionMap;
use spk_schema_foundation::option_map::{host_options, OptionMap};

use crate::{RequirementsList, Result};

/// Describes a resolved build environment in which
/// a binary package may be created.
pub trait Variant {
    fn name(&self) -> Option<&str> {
        None
    }

    /// Input option values for this variant
    fn options(&self) -> Cow<'_, OptionMap>;

    /// Additional requirements for this variant
    fn additional_requirements(&self) -> Cow<'_, RequirementsList>;
}

impl Variant for OptionMap {
    fn options(&self) -> Cow<'_, OptionMap> {
        Cow::Borrowed(self)
    }

    fn additional_requirements(&self) -> Cow<'_, RequirementsList> {
        Cow::Owned(RequirementsList::default())
    }
}

impl<'a, T> Variant for &'a T
where
    T: Variant,
{
    fn name(&self) -> Option<&str> {
        (**self).name()
    }

    fn options(&self) -> Cow<'_, OptionMap> {
        (**self).options()
    }

    fn additional_requirements(&self) -> Cow<'_, RequirementsList> {
        (**self).additional_requirements()
    }
}

/// Allows for extending the values of an existing, opaque variant
pub struct ExtensionVariant<V: Variant> {
    inner: V,
    base_options: OptionMap,
    override_options: OptionMap,
}

impl<V: Variant> From<V> for ExtensionVariant<V> {
    fn from(inner: V) -> Self {
        Self {
            inner,
            base_options: Default::default(),
            override_options: Default::default(),
        }
    }
}

impl<V: Variant> ExtensionVariant<V> {
    /// Add the host options to the base of this variant.
    ///
    /// Variant options will still override and host options
    pub fn with_host_options(mut self, enabled: bool) -> Result<Self> {
        if enabled {
            self.base_options.extend(host_options()?);
        }
        Ok(self)
    }

    /// Add options to override any provided by the base variant
    pub fn with_overrides<O: Into<OptionMap>>(mut self, overrides: O) -> Self {
        self.override_options.extend(overrides.into());
        self
    }
}

impl<V: Variant> Variant for ExtensionVariant<V> {
    fn options(&self) -> Cow<'_, OptionMap> {
        let mut opts = self.base_options.clone();
        opts.extend(self.inner.options().into_owned());
        opts.extend(self.override_options.clone());
        Cow::Owned(opts)
    }

    fn additional_requirements(&self) -> Cow<'_, RequirementsList> {
        self.inner.additional_requirements()
    }
}

impl<V> Clone for ExtensionVariant<V>
where
    V: Variant + Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            base_options: self.base_options.clone(),
            override_options: self.override_options.clone(),
        }
    }
}

impl<V> std::hash::Hash for ExtensionVariant<V>
where
    V: Variant + std::hash::Hash,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
        self.base_options.hash(state);
        self.override_options.hash(state);
    }
}

impl<V> std::cmp::PartialEq for ExtensionVariant<V>
where
    V: Variant + std::cmp::PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
            && self.base_options == other.base_options
            && self.override_options == other.override_options
    }
}

impl<V> std::cmp::Eq for ExtensionVariant<V> where V: Variant + std::cmp::Eq {}

impl<V: Variant> std::fmt::Display for ExtensionVariant<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = self.name() {
            f.write_str("Name: ")?;
            name.fmt(f)?;
            f.write_char('\n')?;
        }
        f.write_str(" Options: ")?;
        f.write_str(&self.options().format_option_map())?;
        let requirements = self.additional_requirements();
        f.write_str("\n Additional Requirements:")?;
        if requirements.len() > 0 {
            for request in requirements.iter() {
                f.write_char('\n')?;
                f.write_str(" - ")?;
                f.write_fmt(format_args!("{request:?}"))?;
            }
        } else {
            f.write_str(" None")?;
        }
        Ok(())
    }
}
