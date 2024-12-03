// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use clap::Args;
use miette::{miette, Result};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::name::OptNameBuf;
use spk_schema::{Recipe, RequirementsList, SpecVariant, Variant as _, VariantExt};

use crate::Error;

#[derive(Clone)]
pub enum VariantSpec {
    /// A variant index
    Index(usize),
    /// A variant filter on one or more options
    Filter(OptionMap),
}

impl FromStr for VariantSpec {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Ok(index) = s.parse() {
            return Ok(VariantSpec::Index(index));
        }
        let s = s.trim();
        if s.starts_with('{') {
            let filter = serde_json::from_str::<OptionMap>(s).map_err(|err| {
                Error::String(format!("failed to parse variant string as json: {err}"))
            })?;
            return Ok(VariantSpec::Filter(filter));
        }
        s.split(',')
            .map(|pair| {
                let (name, value) = pair
                    .split_once('=')
                    .ok_or_else(|| {
                        Error::String(format!(
                            "Invalid option: {pair} (should be in the form name=value)"
                        ))
                    })
                    .and_then(|(name, value)| {
                        Ok((OptNameBuf::try_from(name)?, value.to_string()))
                    })?;
                Ok((name, value))
            })
            .collect::<std::result::Result<OptionMap, Self::Err>>()
            .map(VariantSpec::Filter)
    }
}

/// The location of the definition of a variant of a recipe.
#[derive(Clone, Copy, Debug)]
pub enum VariantLocation {
    /// The variant is defined in the recipe at the given index.
    Index(usize),
    /// The variant was created by command line options at the given
    /// occurrence of `--new-variant`.
    Bespoke(usize),
}

impl std::fmt::Display for VariantLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VariantLocation::Index(i) => write!(f, "variant index {}", i),
            VariantLocation::Bespoke(i) => write!(f, "bespoke variant {}", i),
        }
    }
}

/// A mismatch between the expected and actual values of a variant option.
pub struct VariantOptionMismatch {
    pub expected: String,
    pub actual: Option<String>,
}

/// Details on if a variant is selected for build or why it is not.
pub enum VariantBuildStatus<'r> {
    Enabled(Cow<'r, SpecVariant>),
    /// The variant was filtered out by command line options.
    ///
    /// A mapping of options that mismatched are provided, with the expected
    /// and actual values.
    FilteredOut(HashMap<OptNameBuf, VariantOptionMismatch>),
    /// The variant was a duplicate of a previous variant.
    Duplicate(VariantLocation),
}

/// Information about the variants described by a recipe and if they have
/// been selected for building.
pub struct VariantInfo<'r> {
    pub location: VariantLocation,
    pub build_status: VariantBuildStatus<'r>,
}

#[derive(Default)]
enum VariantInfoIterState<'v, 'r> {
    NotStarted,
    FilteringOnHostOptions(std::iter::Enumerate<std::slice::Iter<'r, spk_schema::SpecVariant>>),
    FilteringOnVariants(std::slice::Iter<'v, VariantSpec>),
    FilteringOnFilter(
        std::slice::Iter<'v, VariantSpec>,
        &'v OptionMap,
        std::iter::Enumerate<std::slice::Iter<'r, spk_schema::SpecVariant>>,
    ),
    FilteringOnNewVariants(std::iter::Enumerate<std::slice::Iter<'v, String>>),
    #[default]
    Invalid,
}

struct VariantInfoIter<'v, 'r, 'o> {
    variant: &'v Variant,
    recipe: &'r spk_schema::SpecRecipe,
    default_variants: &'r [spk_schema::SpecVariant],
    options: &'o OptionMap,
    host_options: Option<&'o OptionMap>,
    host_option_keys: HashSet<&'o OptNameBuf>,
    enabled: HashMap<(OptionMap, RequirementsList), VariantLocation>,
    state: VariantInfoIterState<'v, 'r>,
}

impl<'r> Iterator for VariantInfoIter<'_, 'r, '_> {
    type Item = Result<VariantInfo<'r>>;

    fn next(&mut self) -> Option<Self::Item> {
        let enabled_or_duplicate =
            |v: Cow<'r, SpecVariant>,
             enabled: &mut HashMap<(OptionMap, RequirementsList), VariantLocation>,
             this_location: VariantLocation|
             -> Result<VariantInfo<'r>> {
                let variant_options = (*v)
                    .clone()
                    .with_overrides(self.options.clone())
                    .options()
                    .into_owned();
                // Different additional requirements will make variants with
                // the same options distinct.
                let variant_additional_requirements = (*v).additional_requirements().into_owned();
                match enabled.entry((variant_options, variant_additional_requirements)) {
                    std::collections::hash_map::Entry::Occupied(entry) => Ok(VariantInfo {
                        location: this_location,
                        build_status: VariantBuildStatus::Duplicate(*entry.get()),
                    }),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(this_location);
                        Ok(VariantInfo {
                            location: this_location,
                            build_status: VariantBuildStatus::Enabled(v),
                        })
                    }
                }
            };

        loop {
            match std::mem::take(&mut self.state) {
                VariantInfoIterState::NotStarted => {
                    if self.variant.variants.is_empty() && self.variant.new_variant.is_empty() {
                        self.state = VariantInfoIterState::FilteringOnHostOptions(
                            self.default_variants.iter().enumerate(),
                        );
                    } else {
                        self.state =
                            VariantInfoIterState::FilteringOnVariants(self.variant.variants.iter());
                    }
                }
                VariantInfoIterState::FilteringOnHostOptions(mut iter) => {
                    let (index, v) = iter.next()?;

                    // Even if no filter is specified, variants are still
                    // filtered based on the host options (if any).
                    let variant_options = v.options();
                    let variant_option_keys = variant_options.keys().collect::<HashSet<_>>();
                    let intersecting_host_options =
                        self.host_option_keys.intersection(&variant_option_keys);
                    let mismatched_options: HashMap<_, _> = intersecting_host_options
                        .filter_map(|k| {
                            let expected = self.host_options.unwrap().get(*k).unwrap();
                            let actual = variant_options.get(*k);
                            (Some(expected) != actual).then_some((
                                (*k).clone(),
                                VariantOptionMismatch {
                                    expected: expected.clone(),
                                    actual: actual.cloned(),
                                },
                            ))
                        })
                        .collect();
                    self.state = VariantInfoIterState::FilteringOnHostOptions(iter);
                    if mismatched_options.is_empty() {
                        return Some(enabled_or_duplicate(
                            Cow::Borrowed(v),
                            &mut self.enabled,
                            VariantLocation::Index(index),
                        ));
                    } else {
                        return Some(Ok(VariantInfo {
                            location: VariantLocation::Index(index),
                            build_status: VariantBuildStatus::FilteredOut(mismatched_options),
                        }));
                    }
                }
                VariantInfoIterState::FilteringOnVariants(mut iter) => {
                    let Some(filter) = iter.next() else {
                        self.state = VariantInfoIterState::FilteringOnNewVariants(
                            self.variant.new_variant.iter().enumerate(),
                        );
                        continue;
                    };
                    match filter {
                        VariantSpec::Index(i) if *i < self.default_variants.len() => {
                            self.state = VariantInfoIterState::FilteringOnVariants(iter);
                            return Some(enabled_or_duplicate(
                                Cow::Borrowed(&self.default_variants[*i]),
                                &mut self.enabled,
                                VariantLocation::Index(*i),
                            ));
                        }
                        VariantSpec::Index(i) => {
                            self.state = VariantInfoIterState::FilteringOnVariants(iter);
                            return Some(Err(miette!(
                                "--variant {i} is out of range; {} variant(s) found in {}",
                                self.default_variants.len(),
                                self.recipe.ident().format_ident(),
                            )));
                        }
                        VariantSpec::Filter(filter_options) => {
                            self.state = VariantInfoIterState::FilteringOnFilter(
                                iter,
                                filter_options,
                                self.default_variants.iter().enumerate(),
                            );
                            continue;
                        }
                    }
                }
                VariantInfoIterState::FilteringOnFilter(
                    outer_iter,
                    filter_options,
                    mut default_variants_iter,
                ) => {
                    let Some((index, v)) = default_variants_iter.next() else {
                        self.state = VariantInfoIterState::FilteringOnVariants(outer_iter);
                        continue;
                    };
                    // Variants are filtered based on
                    // the filter options (and host
                    // options) that are set. A variant
                    // is only included if it matches
                    // all the filter options,
                    // and doesn't conflict with the
                    // host options.
                    let variant_options = v.options();
                    let variant_option_keys = variant_options.keys().collect::<HashSet<_>>();
                    let intersecting_host_options =
                        self.host_option_keys.intersection(&variant_option_keys);
                    let mismatched_options: HashMap<_, _> = intersecting_host_options
                        .filter_map(|k| {
                            let expected = self.host_options.unwrap().get(*k).unwrap();
                            let actual = variant_options.get(*k);
                            (Some(expected) != actual).then_some((
                                (*k).clone(),
                                VariantOptionMismatch {
                                    expected: expected.clone(),
                                    actual: actual.cloned(),
                                },
                            ))
                        })
                        .chain(filter_options.iter().filter_map(|(k, v)| {
                            let expected = v;
                            let actual = variant_options.get(k);
                            (Some(expected) != actual).then_some((
                                k.clone(),
                                VariantOptionMismatch {
                                    expected: expected.clone(),
                                    actual: actual.cloned(),
                                },
                            ))
                        }))
                        .collect();
                    self.state = VariantInfoIterState::FilteringOnFilter(
                        outer_iter,
                        filter_options,
                        default_variants_iter,
                    );
                    if mismatched_options.is_empty() {
                        return Some(enabled_or_duplicate(
                            Cow::Borrowed(v),
                            &mut self.enabled,
                            VariantLocation::Index(index),
                        ));
                    } else {
                        return Some(Ok(VariantInfo {
                            location: VariantLocation::Index(index),
                            build_status: VariantBuildStatus::FilteredOut(mismatched_options),
                        }));
                    }
                }
                VariantInfoIterState::FilteringOnNewVariants(mut iter) => {
                    let (index, s) = iter.next()?;
                    self.state = VariantInfoIterState::FilteringOnNewVariants(iter);
                    return Some(
                        serde_json::from_str::<spk_schema::v0::VariantSpec>(s)
                            .map_err(|e| miette!(e))
                            .and_then(|v| {
                                spk_schema::v0::Variant::from_spec(v, &self.recipe.build_options())
                                    .map_err(|e| miette!(e))
                                    .and_then(|v| {
                                        enabled_or_duplicate(
                                            Cow::Owned(spk_schema::SpecVariant::V0(v)),
                                            &mut self.enabled,
                                            VariantLocation::Bespoke(index),
                                        )
                                    })
                            }),
                    );
                }
                VariantInfoIterState::Invalid => {
                    return Some(Err(miette!("Invalid state in VariantInfoIter")));
                }
            }
        }
    }
}

#[derive(Args, Clone)]
pub struct Variant {
    /// Specify a new variant of a package to be built
    ///
    /// When this flag and --variants are not present, the default behavior is
    /// to build/test all the variants of a package, or if a package has no
    /// variants, to build/test the package using its base defaults.
    ///
    /// A new variant is specified as a json value. Anything that would be
    /// accepted in a recipe as a variant entry can be specified here.
    /// For example, `--new-variant '{ "python": "3.9" }'` will build a variant
    /// with python 3.9, ignoring any variants defined in the recipe.
    ///
    /// The `--opt` flag can be used in combination to override the default
    /// value(s) specified in the recipe. Or if a bespoke variant is specified,
    /// `--opt` will still override any value defined in the bespoke variant.
    ///
    /// This flag can be repeated to request multiple builds/tests in the same
    /// run.
    #[clap(long = "new-variant")]
    pub new_variant: Vec<String>,

    /// Specify variants of a package to be built
    ///
    /// When this flag and --new-variant are not present, the default behavior
    /// is to build/test all the variants of a package, or if a package has no
    /// variants, to build/test the package using its base defaults.
    ///
    /// --variant NUM may be used to build a specific variant as defined
    /// in the recipe, by index, starting at 0.
    ///
    /// --variant key=value[,key=value...] may be used to filter on the
    /// variants defined in the recipe. Only variants that specify the same
    /// keys and values as the filter will be built. If a value needs to contain
    /// a comma, a json value can be provided, such as:
    /// `{ "key": "value,with,commas" }`
    ///
    /// This flag can be repeated to request multiple builds/tests in the same
    /// run.
    #[clap(long = "variant")]
    pub variants: Vec<VariantSpec>,
}

impl Variant {
    /// Return an iterator over the variants that have been requested to build
    /// or filtered out by command line options.
    ///
    /// `options` is an OptionMap of all the active overrides, including host
    /// options (if not disabled).
    ///
    /// `host_options` is an OptionMap of all the host options (if not
    /// disabled).
    pub fn requested_variants<'v, 'r, 'o>(
        &'v self,
        recipe: &'r spk_schema::SpecRecipe,
        default_variants: &'r [spk_schema::SpecVariant],
        options: &'o OptionMap,
        host_options: Option<&'o OptionMap>,
    ) -> impl Iterator<Item = Result<VariantInfo<'r>>> + Send + Sync + 'v
    where
        'r: 'v,
        'o: 'v,
    {
        VariantInfoIter::<'v, 'r, 'o> {
            variant: self,
            recipe,
            default_variants,
            options,
            host_options,
            host_option_keys: host_options.map(|o| o.keys().collect()).unwrap_or_default(),
            enabled: HashMap::new(),
            state: VariantInfoIterState::NotStarted,
        }
    }
}
