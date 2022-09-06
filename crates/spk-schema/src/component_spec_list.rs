// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::ComponentSpec;
use crate::foundation::ident_component::Component;

#[cfg(test)]
#[path = "./component_spec_list_test.rs"]
mod component_spec_list_test;

/// A set of packages that are embedded/provided by another.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ComponentSpecList(Vec<ComponentSpec>);

impl ComponentSpecList {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

impl ComponentSpecList {
    /// Collect the names of all components in this list
    pub fn names(&self) -> HashSet<&Component> {
        self.iter().map(|i| &i.name).collect()
    }

    /// Given a set of requested components, resolve the complete list of
    /// components that are needed to satisfy any declared 'uses' dependencies.
    pub fn resolve_uses<'a>(
        &self,
        requests: impl Iterator<Item = &'a Component>,
    ) -> BTreeSet<Component> {
        let by_name = self
            .iter()
            .map(|c| (c.name.clone(), c))
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
                to_visit.append(&mut cmpt.uses.iter().collect())
            }
        }
        // the all component is not a real component that can be used
        visited.remove(&Component::All);
        visited
    }
}

impl Default for ComponentSpecList {
    fn default() -> Self {
        Self(vec![
            ComponentSpec::default_build(),
            ComponentSpec::default_run(),
        ])
    }
}

impl std::ops::Deref for ComponentSpecList {
    type Target = Vec<ComponentSpec>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ComponentSpecList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de> Deserialize<'de> for ComponentSpecList {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ComponentSpecListVisitor;

        impl<'de> serde::de::Visitor<'de> for ComponentSpecListVisitor {
            type Value = ComponentSpecList;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a list of component definitions")
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
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
                while let Some(component) = seq.next_element::<ComponentSpec>()? {
                    if !seen.insert(component.name.clone()) {
                        return Err(serde::de::Error::custom(format!(
                            "found multiple components with the name '{}'",
                            component.name
                        )));
                    }
                    components.push(component)
                }

                // we guarantee that these default components are
                // present in all specs, using a default setup if needed
                if !seen.contains(&Component::Build) {
                    components.push(ComponentSpec::default_build());
                }
                if !seen.contains(&Component::Run) {
                    components.push(ComponentSpec::default_run());
                }

                if seen.contains(&Component::All) {
                    return Err(serde::de::Error::custom(
                        "The 'all' component is reserved, and cannot be defined in a spec"
                            .to_string(),
                    ));
                }

                // all referenced components must have been defined
                // within the spec as well
                for component in components.iter() {
                    for name in component.uses.iter() {
                        if !seen.contains(name) {
                            return Err(serde::de::Error::custom(format!(
                                "component '{}' uses '{}', but it does not exist",
                                component.name, name
                            )));
                        }
                    }
                }

                components.sort_by(|a, b| a.name.cmp(&b.name));
                Ok(ComponentSpecList(components))
            }
        }

        deserializer.deserialize_seq(ComponentSpecListVisitor)
    }
}
