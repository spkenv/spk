// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};

use super::RecipeOption;

#[cfg(test)]
#[path = "./recipe_option_list_test.rs"]
mod recipe_option_list_test;

// Recipe options define the set of dependencies and inputs variables
// to the package build process. This represents the complete set of
// options defined for a recipe.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RecipeOptionList(Vec<RecipeOption>);

impl RecipeOptionList {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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

            fn visit_unit<E>(self) -> Result<Self::Value, E>
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
