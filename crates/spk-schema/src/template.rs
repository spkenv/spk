// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::str::FromStr;

use bracoxide::OxidizationError;
use bracoxide::tokenizer::TokenizationError;
use serde::de::value::MapAccessDeserializer;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::{OptionMap, Stringified};
use spk_schema_foundation::version::Version;

use crate::{Error, Result, SpecFileData};

/// A recipe template for building multiple versions of a package.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TemplateSpec {
    /// Describes the versions that this template can produce.
    ///
    /// If none are specified, the package must have a single, hard-coded version
    /// that can be parsed.
    pub versions: TemplateVersions,
}

impl TemplateSpec {
    /// Constructs a template specification that can only
    /// be used to produce a single pre-defined version.
    pub fn from_single_version(version: Version) -> Self {
        Self {
            versions: TemplateVersions {
                in_spec: Some(version),
                allowed: None,
                discover: None,
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TemplateVersions {
    /// The version specified in the template's `pkg` field, if any.
    ///
    /// This field will also be empty if the value could not be parsed
    /// as a version, which is common for templates as they typically
    /// inject the value dynamically via `{{ version }}`.
    #[serde(skip)]
    pub in_spec: Option<Version>,
    /// Manually specified version numbers, with support for brace expansion of ranges.
    #[serde(default, rename = "static", skip_serializing_if = "Option::is_none")]
    pub allowed: Option<OrderedVersionSet>,
    /// Automatically discovered versions
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discover: Option<DiscoverStrategy>,
}

impl DiscoverVersions for TemplateVersions {
    fn discover_versions(&self) -> Result<BTreeSet<Version>> {
        let mut versions = BTreeSet::new();
        if let Some(in_spec) = &self.in_spec {
            versions.insert(in_spec.clone());
        }
        if let Some(allowed) = self.allowed.as_ref() {
            versions.extend(allowed.0.iter().cloned());
        }
        if let Some(discover) = self.discover.as_ref() {
            versions.extend(discover.discover_versions()?);
        }
        Ok(versions)
    }
}

/// A set of manually specified versions.
///
/// When deserializing, this will accept either a single string or
/// list of strings, and each one can contain one or more brace
/// expansion patterns.
///
/// ```
/// let versions: spk_schema::OrderedVersionSet = serde_yaml::from_str(r#"['1.0.{1..5}', '1.1.{1..3}']"#).unwrap();
/// assert_eq!(versions, spk_schema::OrderedVersionSet(std::collections::BTreeSet::from_iter(vec![
///     spk_schema::version!("1.0.1"),
///     spk_schema::version!("1.0.2"),
///     spk_schema::version!("1.0.3"),
///     spk_schema::version!("1.0.4"),
///     spk_schema::version!("1.0.5"),
///     spk_schema::version!("1.1.1"),
///     spk_schema::version!("1.1.2"),
///     spk_schema::version!("1.1.3"),
/// ])));
/// ```
#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Default)]
pub struct OrderedVersionSet(pub BTreeSet<Version>);

impl<'de> serde::de::Deserialize<'de> for OrderedVersionSet {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct OrderedVersionSetVisitor;

        impl<'de> serde::de::Visitor<'de> for OrderedVersionSetVisitor {
            type Value = OrderedVersionSet;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter
                    .write_str("a single or list of version numbers with optional brace expansions")
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let mut versions = BTreeSet::new();
                let expand_result = bracoxide::bracoxidize(v);
                let expanded = match expand_result {
                    Ok(expanded) => expanded,
                    Err(OxidizationError::TokenizationError(TokenizationError::NoBraces))
                    | Err(OxidizationError::TokenizationError(TokenizationError::EmptyContent))
                    | Err(OxidizationError::TokenizationError(
                        TokenizationError::FormatNotSupported,
                    )) => {
                        vec![v.to_owned()]
                    }
                    Err(err) => {
                        return Err(serde::de::Error::custom(format!(
                            "invalid brace expansion: {err:?}"
                        )));
                    }
                };
                for version in expanded {
                    let parsed = Version::from_str(&version).map_err(|err| {
                        serde::de::Error::custom(format!(
                            "bad brace expansion or invalid version '{version}': {err}"
                        ))
                    })?;
                    versions.insert(parsed);
                }
                Ok(OrderedVersionSet(versions))
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut versions = BTreeSet::new();
                while let Some(version_expr) = seq.next_element()? {
                    versions.append(&mut OrderedVersionSetVisitor.visit_str(version_expr)?.0);
                }
                Ok(OrderedVersionSet(versions))
            }
        }

        deserializer.deserialize_any(OrderedVersionSetVisitor)
    }
}

impl serde::Serialize for OrderedVersionSet {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if self.0.len() == 1 {
            serializer.serialize_str(&self.0.iter().next().unwrap().to_string())
        } else {
            let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
            for version in &self.0 {
                seq.serialize_element(version)?;
            }
            seq.end()
        }
    }
}

/// The strategy for discovering versions.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[enum_dispatch::enum_dispatch(DiscoverVersions)]
pub enum DiscoverStrategy {
    GitTags(GitTagsDiscovery),
}

impl<'de> Deserialize<'de> for DiscoverStrategy {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct DiscoverStrategyVisitor;

        impl<'de> serde::de::Visitor<'de> for DiscoverStrategyVisitor {
            type Value = DiscoverStrategy;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a version discovery method")
            }

            fn visit_map<A>(self, map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut peekable = serde_peekable::PeekableMapAccess::from(map);
                let first_key = peekable.peek_key::<Stringified>()?;
                let Some(first_key) = first_key else {
                    return Err(serde::de::Error::missing_field("pkg or var"));
                };
                match first_key.as_ref() {
                    "gitTags" => Ok(DiscoverStrategy::GitTags(Deserialize::deserialize(
                        MapAccessDeserializer::new(peekable),
                    )?)),
                    _ => Err(serde::de::Error::custom(
                        "expected 'gitTags' as the first key",
                    )),
                }
            }
        }

        deserializer.deserialize_any(DiscoverStrategyVisitor)
    }
}

/// Discover versions from git tags.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitTagsDiscovery {
    #[serde(with = "serde_regex")]
    pub git_tags: Vec<regex::Regex>,
    pub url: url::Url,
    #[serde(default, with = "serde_regex", skip_serializing_if = "Vec::is_empty")]
    pub extract: Vec<regex::Regex>,
}

impl DiscoverVersions for GitTagsDiscovery {
    fn discover_versions(&self) -> Result<BTreeSet<Version>> {
        let url = &self.url;

        let output = std::process::Command::new("git")
            .args(["ls-remote", "--tags", "--quiet", "--refs"])
            .arg(url.as_str())
            .output()
            .map_err(Error::GitCommandFailed)?;

        if !output.status.success() {
            return Err(Error::GitCommandExited(
                output.status.code().unwrap_or(1),
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let refs = stdout
            .lines()
            // git ls-remote --tags outputs in the form OID <tab> refs/tags/<name>
            .filter_map(|line| {
                line.split_once('\t')
                    .and_then(|(_oid, tag)| tag.strip_prefix("refs/tags/"))
            })
            // ensure that we only retain tags that match the specified patterns
            .filter(|ref_name| {
                self.git_tags.is_empty() || self.git_tags.iter().any(|re| re.is_match(ref_name))
            })
            // extract any specified capture group
            .filter_map(|ref_name| {
                if self.extract.is_empty() {
                    return Some(ref_name);
                }
                for pattern in self.extract.iter() {
                    let Some(groups) = pattern.captures(ref_name) else {
                        continue;
                    };
                    if let Some(extracted) = groups.get(1) {
                        return Some(extracted.as_str());
                    }
                }
                None
            })
            .map(|v| Version::from_str(v).map_err(Error::FailedToParseTagAsVersion));

        refs.collect()
    }
}

#[enum_dispatch::enum_dispatch]
pub trait DiscoverVersions {
    fn discover_versions(&self) -> Result<BTreeSet<Version>>;
}

/// Can be rendered into a recipe.
#[enum_dispatch::enum_dispatch]
pub trait Template: Sized {
    /// Identify the location of this template on disk
    fn file_path(&self) -> &Path;

    /// Render this template to a string with the provided values.
    fn render_to_string(&self, data: TemplateRenderConfig) -> Result<String>;

    /// Render this template with the provided values and parse the output.
    fn render(&self, data: TemplateRenderConfig) -> Result<SpecFileData> {
        let rendered = self.render_to_string(data)?;
        SpecFileData::from_yaml(rendered)
    }
}

pub trait TemplateExt: Template {
    /// Load this template from a file on disk
    fn from_file(path: &Path) -> Result<Self>;
}

/// Used to configure aspects of how a template will be rendered.
#[derive(Debug, Default, Clone)]
pub struct TemplateRenderConfig {
    /// The version of the package to build.
    ///
    /// Exposed via the `version` variable in templates.
    ///
    /// If given, this version must be allowed by the template
    /// and will be validated. If not given, the template must
    /// either have a clear default or only be setup to build
    /// a single version.
    pub version: Option<Version>,
    /// Additional options for the template rendering.
    ///
    /// These are exposed as the `opt` variable in templates
    pub options: OptionMap,
    /// Sets environment variables for the template data.
    ///
    /// These values will override any that are actually in the environment
    /// when rendering. Exposed via the `env` variable in templates.
    pub environment: HashMap<String, String>,
}

impl TemplateRenderConfig {
    /// Sets the package version for the template data.
    pub fn with_version(mut self, version: Version) -> Self {
        self.version = Some(version);
        self
    }

    /// Sets additional options for the template data.
    pub fn with_options<I, T>(mut self, opt: I) -> Self
    where
        OptionMap: Extend<T>,
        I: IntoIterator<Item = T>,
    {
        self.options.extend(opt);
        self
    }

    /// Sets environment variables for the template data.
    ///
    /// These values will override any that are actually in the environment
    /// when rendering.
    pub fn with_environment_vars<I, T>(mut self, env: I) -> Self
    where
        HashMap<String, String>: Extend<T>,
        I: IntoIterator<Item = T>,
    {
        self.environment.extend(env);
        self
    }
}

/// The structured data that should be made available
/// when rendering spk templates into recipes
#[derive(serde::Serialize, Debug, Clone)]
pub struct TemplateData {
    /// Information about the release of spk being used
    spk: SpkInfo,
    /// The version of the package being built
    version: Version,
    /// The option values for this template, expanded
    /// from an option map so that namespaced options
    /// like `python.abi` actually live under the `python`
    /// field rather than as a field with a '.' in the name
    opt: serde_yaml::Mapping,
    /// Environment variable data for the current process
    env: HashMap<String, String>,
}

impl TemplateData {
    /// Create the set of templating data for the current process and options
    pub fn new(version: Version, options: OptionMap, mut env: HashMap<String, String>) -> Self {
        for (k, v) in std::env::vars() {
            env.entry(k).or_insert(v);
        }
        TemplateData {
            spk: SpkInfo::default(),
            version,
            opt: options.into_yaml_value_expanded(),
            env,
        }
    }
}

/// The structured data that should be made available
/// when rendering spk templates into recipes
#[derive(serde::Serialize, Debug, Clone)]
struct SpkInfo {
    version: &'static str,
}

impl Default for SpkInfo {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

mod serde_regex {
    use std::str::FromStr;

    use serde::Serialize;
    use serde::ser::SerializeSeq;

    struct RegexVisitor;

    impl<'de> serde::de::Visitor<'de> for RegexVisitor {
        type Value = Vec<regex::Regex>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a single or array of regular expression strings")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            regex::Regex::from_str(v)
                .map_err(serde::de::Error::custom)
                .map(|regex| vec![regex])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut regexes = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(regex) = seq.next_element()? {
                regexes.push(regex::Regex::from_str(regex).map_err(serde::de::Error::custom)?);
            }
            Ok(regexes)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Vec<regex::Regex>, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_any(RegexVisitor)
    }

    pub fn serialize<S>(
        value: &[regex::Regex],
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        if value.len() == 1 {
            value[0].as_str().serialize(serializer)
        } else {
            let mut seq = serializer.serialize_seq(Some(value.len()))?;
            for regex in value {
                seq.serialize_element(regex.as_str())?;
            }
            seq.end()
        }
    }
}
