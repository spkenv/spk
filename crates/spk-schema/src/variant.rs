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

pub trait VariantExt
where
    Self: Sized,
{
    /// Add the host options to the base of this variant.
    ///
    /// Variant options will still override and host options
    fn with_host_options(self) -> Result<Override<Self>>;

    /// Add options to override any provided by the base variant
    fn with_overrides<O: Into<OptionMap>>(self, overrides: O) -> Override<Self>;
}

impl<T> VariantExt for T
where
    T: Variant,
{
    /// Add the host options to the base of this variant.
    ///
    /// Variant options will still override and host options
    fn with_host_options(self) -> Result<Override<Self>> {
        Ok(self.with_overrides(host_options()?))
    }

    /// Add options to override any provided by the base variant
    fn with_overrides<O: Into<OptionMap>>(self, overrides: O) -> Override<T> {
        Override {
            inner: self,
            overrides: overrides.into(),
        }
    }
}

/// The type returned by [`VariantExt::with_overrides`].
pub struct Override<T> {
    inner: T,
    overrides: OptionMap,
}

impl<T> Variant for Override<T>
where
    T: Variant,
{
    fn name(&self) -> Option<&str> {
        self.inner.name()
    }

    fn options(&self) -> Cow<'_, OptionMap> {
        let mut opts = self.inner.options().into_owned();
        opts.extend(self.overrides.clone());
        Cow::Owned(opts)
    }

    fn additional_requirements(&self) -> Cow<'_, RequirementsList> {
        self.inner.additional_requirements()
    }
}

impl<V> Clone for Override<V>
where
    V: Variant + Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            overrides: self.overrides.clone(),
        }
    }
}

impl<V> std::hash::Hash for Override<V>
where
    V: Variant + std::hash::Hash,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
        self.overrides.hash(state);
    }
}

impl<V> std::cmp::PartialEq for Override<V>
where
    V: Variant + std::cmp::PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner && self.overrides == other.overrides
    }
}

impl<V> std::cmp::Eq for Override<V> where V: Variant + std::cmp::Eq {}

impl<V: Variant> std::fmt::Display for Override<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let br = if f.alternate() { ' ' } else { '\n' };
        let pad = if f.alternate() { "" } else { "  " };
        if let Some(name) = self.name() {
            f.write_str("Name: ")?;
            name.fmt(f)?;
            f.write_char(br)?;
        }
        f.write_str("Options: ")?;
        f.write_str(&self.options().format_option_map())?;
        let requirements = self.additional_requirements();
        f.write_fmt(format_args!("{br}Additional Requirements:"))?;
        if requirements.len() > 0 {
            for request in requirements.iter() {
                f.write_char(br)?;
                f.write_str(pad)?;
                f.write_fmt(format_args!("{request:#}"))?;
            }
        } else {
            f.write_fmt(format_args!("{pad}None"))?;
        }
        Ok(())
    }
}
