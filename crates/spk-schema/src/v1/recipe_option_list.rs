// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::collections::btree_map::Entry;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use spk_schema_foundation::name::OptNameBuf;
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::version::Compatibility;

use super::RecipeOption;
use crate::{Error, Result};

#[cfg(test)]
#[path = "./recipe_option_list_test.rs"]
mod recipe_option_list_test;

// Recipe options define the set of dependencies and inputs variables
// to the package build process. This represents the complete set of
// options defined for a recipe.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RecipeOptionList(Vec<RecipeOption>);

impl RecipeOptionList {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Resolve the values for all active options given the
    /// desired inputs/variant.
    pub fn resolve(&self, given: &OptionMap) -> Result<OptionMap> {
        let mut all = Cow::Borrowed(given);
        let mut resolved = OptionMap::default();
        let mut something_changed = true;
        let mut previous_contention = HashMap::new();
        while something_changed {
            let mut contention = HashMap::<OptNameBuf, HashSet<String>>::new();
            something_changed = false;
            for opt in self.iter() {
                let Some(value) = opt.value(given.get(opt.name())) else {
                    continue;
                };
                if let Compatibility::Incompatible { reason } = opt.validate(&value) {
                    return Err(Error::String(reason));
                }
                let value = value.clone();
                match all.to_mut().entry(opt.name().to_owned()) {
                    Entry::Vacant(entry) => {
                        something_changed = true;
                        entry.insert(value.clone());
                    }
                    Entry::Occupied(mut entry) => {
                        if entry.get() != &value {
                            something_changed = true;
                            entry.insert(value.clone());
                        }
                    }
                }
                match resolved.entry(opt.name().to_owned()) {
                    Entry::Vacant(entry) => {
                        entry.insert(value.clone());
                    }
                    Entry::Occupied(entry) if entry.get() != &value => {
                        // do not allow the loop to exit, as we had multiple
                        // options providing a different value for the same variable
                        // (handled later on)
                        something_changed = true;
                        // we explicitly do not set the entry to any new value here
                        // to avoid a situation where options might toggle each other
                        // back and forth, creating an infinite loop
                        let seen = contention.entry(entry.key().to_owned()).or_default();
                        seen.insert(value);
                        seen.insert(entry.get().to_string());
                    }
                    Entry::Occupied(_) => {}
                }
            }
            if !contention.is_empty() {
                if contention != previous_contention {
                    // although there was contention between the options,
                    // it's not a repeated issue and may still converge on the
                    // next iteration
                    previous_contention = contention;
                    continue;
                }
                return Err(Error::MultipleOptionValuesResolved {
                    resolved: contention,
                });
            }
        }
        Ok(resolved)
    }
}

impl std::ops::Deref for RecipeOptionList {
    type Target = Vec<RecipeOption>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for RecipeOptionList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de> Deserialize<'de> for RecipeOptionList {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OptionListVisitor;

        impl<'de> serde::de::Visitor<'de> for OptionListVisitor {
            type Value = RecipeOptionList;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a list of package options")
            }

            fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(RecipeOptionList::default())
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let size_hint = seq.size_hint().unwrap_or(0);
                let mut options = Vec::with_capacity(size_hint);
                while let Some(option) = seq.next_element()? {
                    options.push(option)
                }
                Ok(RecipeOptionList(options))
            }
        }

        deserializer.deserialize_seq(OptionListVisitor)
    }
}
