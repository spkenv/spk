// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{BTreeSet, HashMap, HashSet};
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::ComponentSpec;
use crate::foundation::ident_component::Component;
use crate::{ComponentFileMatchMode, Package};

#[cfg(test)]
#[path = "./component_spec_list_test.rs"]
mod component_spec_list_test;

/// A set of packages that are embedded/provided by another.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ComponentSpecList<EmbeddedStub, Spec = ComponentSpec<EmbeddedStub>>(
    Vec<Spec>,
    PhantomData<EmbeddedStub>,
)
where
    Spec: AsRef<ComponentSpec<EmbeddedStub>>;

impl<EmbeddedStub, Spec> ComponentSpecList<EmbeddedStub, Spec>
where
    Spec: AsRef<ComponentSpec<EmbeddedStub>> + From<ComponentSpec<EmbeddedStub>> + PartialEq,
{
    pub fn is_default(&self) -> bool
    where
        EmbeddedStub: PartialEq,
    {
        self == &Self::default()
    }
}

impl<EmbeddedStub, Spec> ComponentSpecList<EmbeddedStub, Spec>
where
    Spec: AsRef<ComponentSpec<EmbeddedStub>>,
{
    /// Collect the names of all components in this list
    pub fn names(&self) -> HashSet<&Component> {
        self.iter().map(|i| &i.as_ref().name).collect()
    }

    /// Collect a copy of the names of all components in this list
    pub fn names_owned(&self) -> BTreeSet<Component> {
        self.iter().map(|i| &i.as_ref().name).cloned().collect()
    }

    /// Given a set of requested components, resolve the complete list of
    /// components that are needed to satisfy any declared 'uses' dependencies.
    ///
    /// This function assumes that all the `uses` values identify real components
    /// defined in this list, and will silently ignore any that are used but missing.
    pub fn resolve_uses<'a>(
        &self,
        requests: impl IntoIterator<Item = &'a Component>,
    ) -> impl Iterator<Item = &ComponentSpec<EmbeddedStub>> + '_ {
        let all = self.resolve_uses_names(requests);
        self.0
            .iter()
            .map(AsRef::as_ref)
            .filter(move |c| all.contains(&c.name))
    }

    /// Like [`Self::resolve_uses`] but only return the names of
    /// the components that are needed.
    pub fn resolve_uses_names<'a>(
        &self,
        requests: impl IntoIterator<Item = &'a Component>,
    ) -> BTreeSet<Component> {
        let by_name = self
            .iter()
            .map(|c| (c.as_ref().name.clone(), c))
            .collect::<HashMap<_, _>>();
        let mut to_visit = requests.into_iter().collect::<Vec<_>>();
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
                to_visit.extend(cmpt.as_ref().uses.iter())
            }
        }
        // the all component is not a real component that can be used
        visited.remove(&Component::All);
        visited
    }
}

impl<EmbeddedStub> ComponentSpecList<EmbeddedStub> {
    /// Convert the embedded stub type stored in these component specs
    pub fn map_embedded_stubs<F, T>(self, convert: F) -> ComponentSpecList<T>
    where
        T: Package,
        F: Fn(EmbeddedStub) -> T,
    {
        ComponentSpecList(
            self.0
                .into_iter()
                .map(|component_spec| ComponentSpec::<T> {
                    name: component_spec.name,
                    files: component_spec.files,
                    uses: component_spec.uses,
                    requirements: component_spec.requirements,
                    embedded: component_spec.embedded.into_iter().map(&convert).collect(),
                    file_match_mode: component_spec.file_match_mode,
                })
                .collect(),
            PhantomData,
        )
    }
}

impl<EmbeddedStub, Spec> Default for ComponentSpecList<EmbeddedStub, Spec>
where
    Spec: AsRef<ComponentSpec<EmbeddedStub>> + From<ComponentSpec<EmbeddedStub>>,
{
    fn default() -> Self {
        Self(
            vec![
                Spec::from(ComponentSpec::default_build()),
                Spec::from(ComponentSpec::default_run()),
            ],
            PhantomData,
        )
    }
}

impl<EmbeddedStub, Spec> std::ops::Deref for ComponentSpecList<EmbeddedStub, Spec>
where
    Spec: AsRef<ComponentSpec<EmbeddedStub>>,
{
    type Target = Vec<Spec>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<EmbeddedStub, Spec> std::ops::DerefMut for ComponentSpecList<EmbeddedStub, Spec>
where
    Spec: AsRef<ComponentSpec<EmbeddedStub>>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<EmbeddedStub> FromIterator<ComponentSpec<EmbeddedStub>> for ComponentSpecList<EmbeddedStub> {
    fn from_iter<T: IntoIterator<Item = ComponentSpec<EmbeddedStub>>>(iter: T) -> Self {
        Self(FromIterator::from_iter(iter), PhantomData)
    }
}

impl<'de, EmbeddedStub, Spec> Deserialize<'de> for ComponentSpecList<EmbeddedStub, Spec>
where
    Spec: serde::de::DeserializeOwned
        + From<ComponentSpec<EmbeddedStub>>
        + AsRef<ComponentSpec<EmbeddedStub>>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ComponentSpecListVisitor<EmbeddedStub, Spec>(
            PhantomData<dyn Fn() -> (EmbeddedStub, Spec)>,
        );

        impl<'de, EmbeddedStub, Spec> serde::de::Visitor<'de>
            for ComponentSpecListVisitor<EmbeddedStub, Spec>
        where
            Spec: serde::de::DeserializeOwned
                + From<ComponentSpec<EmbeddedStub>>
                + AsRef<ComponentSpec<EmbeddedStub>>,
        {
            type Value = ComponentSpecList<EmbeddedStub, Spec>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a list of component definitions")
            }

            fn visit_unit<Err>(self) -> Result<Self::Value, Err>
            where
                Err: serde::de::Error,
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
                while let Some(component) = seq.next_element::<Spec>()? {
                    if !seen.insert(component.as_ref().name.clone()) {
                        return Err(serde::de::Error::custom(format!(
                            "found multiple components with the name '{}'",
                            component.as_ref().name
                        )));
                    }
                    components.push(component)
                }

                // we guarantee that these default components are
                // present in all specs, using a default setup if needed
                if !seen.contains(&Component::Build) {
                    components.push(Spec::from(ComponentSpec::default_build()));
                    seen.insert(Component::Build);
                }
                if !seen.contains(&Component::Run) {
                    components.push(Spec::from(ComponentSpec::default_run()));
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
                for component in components.iter().map(AsRef::as_ref) {
                    if matches!(component.file_match_mode, ComponentFileMatchMode::Remaining) {
                        using_exclusive_filter_mode = true;
                    }

                    for name in component.uses.iter() {
                        if !seen.contains(name) {
                            return Err(serde::de::Error::custom(format!(
                                "component '{}' uses '{name}', but it does not exist",
                                component.name
                            )));
                        }
                    }
                }

                // when using Exclusive filter mode, the order has meaning and
                // the components order must be preserved
                if !using_exclusive_filter_mode {
                    components.sort_by(|a, b| a.as_ref().name.cmp(&b.as_ref().name));
                }

                Ok(ComponentSpecList(components, PhantomData))
            }
        }

        deserializer.deserialize_seq(ComponentSpecListVisitor::<EmbeddedStub, Spec>(PhantomData))
    }
}
