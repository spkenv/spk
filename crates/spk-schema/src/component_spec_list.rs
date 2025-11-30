// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::spec_ops::{ComponentFileMatchMode, ComponentOps, HasBuildIdent};

use super::ComponentSpec;
use crate::foundation::ident_component::Component;
use crate::{RecipeComponentSpec, Result};

#[cfg(test)]
#[path = "./component_spec_list_test.rs"]
mod component_spec_list_test;

/// A set of packages that are embedded/provided by another.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ComponentSpecList<ComponentT>(Vec<ComponentT>);

impl ComponentSpecList<RecipeComponentSpec> {
    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<K, R>(
        self,
        options: &OptionMap,
        resolved_by_name: &HashMap<K, R>,
    ) -> Result<ComponentSpecList<ComponentSpec>>
    where
        K: Eq + std::hash::Hash,
        K: std::borrow::Borrow<PkgName>,
        R: HasBuildIdent,
    {
        Ok(ComponentSpecList(
            self.0
                .into_iter()
                .map(|component| component.render_all_pins(options, resolved_by_name))
                .collect::<crate::Result<Vec<_>>>()?,
        ))
    }
}

impl<ComponentSpecT> IsDefault for ComponentSpecList<ComponentSpecT>
where
    ComponentSpecT: ComponentSpecDefaults + PartialEq,
{
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

impl<ComponentSpecT> ComponentSpecList<ComponentSpecT>
where
    ComponentSpecT: ComponentOps,
{
    /// Collect the names of all components in this list
    pub fn names(&self) -> HashSet<&Component> {
        self.iter().map(|i| i.name()).collect()
    }

    /// Given a set of requested components, resolve the complete list of
    /// components that are needed to satisfy any declared 'uses' dependencies.
    pub fn resolve_uses<'a>(
        &self,
        requests: impl Iterator<Item = &'a Component>,
    ) -> BTreeSet<Component> {
        let by_name = self
            .iter()
            .map(|c| (c.name().clone(), c))
            .collect::<HashMap<_, _>>();
        let mut to_visit = requests.collect::<Vec<_>>();
        let mut visited = BTreeSet::new();

        while let Some(requested) = to_visit.pop() {
            if visited.contains(requested) {
                continue;
            }
            visited.insert(requested.clone());
            if requested.is_all() {
                to_visit.append(&mut by_name.keys().collect())
            }
            if let Some(cmpt) = by_name.get(requested) {
                to_visit.append(&mut cmpt.uses().iter().collect())
            }
        }
        // the all component is not a real component that can be used
        visited.remove(&Component::All);
        visited
    }

    /// Retrieve the component with the provided name
    pub fn get<C>(&self, name: C) -> Option<&ComponentSpecT>
    where
        C: std::cmp::PartialEq<Component>,
    {
        self.iter().find(|c| name == *c.name())
    }

    /// Retrieve a component with the provided name or build and insert new one
    pub fn get_or_insert_with(
        &mut self,
        name: Component,
        default: impl FnOnce() -> ComponentSpecT,
    ) -> &mut ComponentSpecT {
        let position = match self.iter().position(|c| *c.name() == name) {
            Some(p) => p,
            None => {
                self.push(default());
                self.len() - 1
            }
        };
        &mut self[position]
    }
}

pub(crate) trait ComponentSpecDefaults {
    fn default_build() -> Self;
    fn default_run() -> Self;
}

impl<ComponentSpecT> Default for ComponentSpecList<ComponentSpecT>
where
    ComponentSpecT: ComponentSpecDefaults,
{
    fn default() -> Self {
        Self(vec![
            ComponentSpecT::default_build(),
            ComponentSpecT::default_run(),
        ])
    }
}

impl<ComponentSpecT> std::ops::Deref for ComponentSpecList<ComponentSpecT> {
    type Target = Vec<ComponentSpecT>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<ComponentSpecT> std::ops::DerefMut for ComponentSpecList<ComponentSpecT> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de, ComponentSpecT> Deserialize<'de> for ComponentSpecList<ComponentSpecT>
where
    ComponentSpecT: ComponentOps + ComponentSpecDefaults + DeserializeOwned + Serialize,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ComponentSpecListVisitor<ComponentSpecT> {
            _marker: std::marker::PhantomData<ComponentSpecT>,
        }

        impl<'de, ComponentSpecT> serde::de::Visitor<'de> for ComponentSpecListVisitor<ComponentSpecT>
        where
            ComponentSpecT: ComponentOps + ComponentSpecDefaults + DeserializeOwned + Serialize,
        {
            type Value = ComponentSpecList<ComponentSpecT>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a list of component definitions")
            }

            fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(ComponentSpecList::default())
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let size_hint = seq.size_hint().unwrap_or(0);
                let mut seen = std::collections::HashSet::with_capacity(size_hint);
                let mut components = Vec::with_capacity(size_hint);
                while let Some(component) = seq.next_element::<ComponentSpecT>()? {
                    let component_name = ComponentOps::name(&component);
                    if !seen.insert(component_name.clone()) {
                        return Err(serde::de::Error::custom(format!(
                            "found multiple components with the name '{component_name}'",
                        )));
                    }
                    components.push(component)
                }

                // we guarantee that these default components are
                // present in all specs, using a default setup if needed
                if !seen.contains(&Component::Build) {
                    components.push(ComponentSpecT::default_build());
                    seen.insert(Component::Build);
                }
                if !seen.contains(&Component::Run) {
                    components.push(ComponentSpecT::default_run());
                    seen.insert(Component::Run);
                }

                if seen.contains(&Component::All) {
                    return Err(serde::de::Error::custom(
                        "The 'all' component is reserved, and cannot be defined in a spec"
                            .to_string(),
                    ));
                }

                let mut using_exclusive_filter_mode = false;

                // all referenced components must have been defined
                // within the spec as well
                for component in components.iter() {
                    if matches!(
                        component.file_match_mode(),
                        ComponentFileMatchMode::Remaining
                    ) {
                        using_exclusive_filter_mode = true;
                    }

                    for name in component.uses().iter() {
                        if !seen.contains(name) {
                            return Err(serde::de::Error::custom(format!(
                                "component '{}' uses '{name}', but it does not exist",
                                ComponentOps::name(component)
                            )));
                        }
                    }
                }

                // when using Exclusive filter mode, the order has meaning and
                // the components order must be preserved
                if !using_exclusive_filter_mode {
                    components.sort_by(|a, b| a.name().cmp(b.name()));
                }

                Ok(ComponentSpecList(components))
            }
        }

        deserializer.deserialize_seq(ComponentSpecListVisitor {
            _marker: std::marker::PhantomData,
        })
    }
}

impl From<ComponentSpecList<ComponentSpec>> for ComponentSpecList<RecipeComponentSpec> {
    fn from(value: ComponentSpecList<ComponentSpec>) -> Self {
        ComponentSpecList(value.0.into_iter().map(Into::into).collect())
    }
}
