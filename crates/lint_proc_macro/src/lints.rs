// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use ngrammatic::CorpusBuilder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub enum LintKind {
    UnknownV0SpecKey,
    UnknownBuildSpecKey,
    UnknownInstallSpecKey,
    UnknownEnvOpKey,
    UnknownSourceSpecKey,
    UnknownTestSpecKey,
    UnknownMetaSpecKey,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub enum LintMessage {
    UnknownV0SpecKey(V0SpecKey),
    UnknownBuildSpecKey(BuildSpecKey),
    UnknownInstallSpecKey(InstallSpecKey),
    UnknownEnvOpKey(EnvOpKey),
    UnknownSourceSpecKey(SourceSpecKey),
    UnknownTestSpecKey(TestSpecKey),
    UnknownMetaSpecKey(MetaSpecKey),
}

impl LintMessage {
    pub fn message(&self) -> String {
        match self {
            Self::UnknownV0SpecKey(key) => key.message.clone(),
            Self::UnknownBuildSpecKey(key) => key.message.clone(),
            Self::UnknownInstallSpecKey(key) => key.message.clone(),
            Self::UnknownEnvOpKey(key) => key.message.clone(),
            Self::UnknownSourceSpecKey(key) => key.message.clone(),
            Self::UnknownTestSpecKey(key) => key.message.clone(),
            Self::UnknownMetaSpecKey(key) => key.message.clone(),
        }
    }
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct V0SpecKey {
    key: String,
    message: String,
}

impl V0SpecKey {
    pub fn new(unknown_key: &str) -> Self {
        let mut message = format!("Unrecognized V0 Spec key: {unknown_key}. ");
        let mut corpus = CorpusBuilder::new().finish();

        corpus.add_text("pkg");
        corpus.add_text("meta");
        corpus.add_text("compat");
        corpus.add_text("deprecated");
        corpus.add_text("sources");
        corpus.add_text("build");
        corpus.add_text("tests");
        corpus.add_text("install");
        corpus.add_text("api");

        match corpus.search(unknown_key, 0.6).first() {
            Some(s) => message.push_str(format!("(Did you mean: '{}'?)", s.text).as_str()),
            None => {
                message.push_str(format!("(No similar keys found for: {}.)", unknown_key).as_str())
            }
        };

        Self {
            key: std::mem::take(&mut unknown_key.to_string()),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct EnvOpKey {
    key: String,
    message: String,
}

impl EnvOpKey {
    pub fn new(unknown_key: &str) -> Self {
        let mut message = format!("Unrecognized EnvOp key: {unknown_key}. ");
        let mut corpus = CorpusBuilder::new().finish();

        corpus.add_text("append");
        corpus.add_text("comment");
        corpus.add_text("prepend");
        corpus.add_text("priority");
        corpus.add_text("set");

        match corpus.search(unknown_key, 0.6).first() {
            Some(s) => message.push_str(format!("(Did you mean: '{}'?)", s.text).as_str()),
            None => {
                message.push_str(format!("(No similar keys found for: {}.)", unknown_key).as_str())
            }
        };
        Self {
            key: std::mem::take(&mut unknown_key.to_string()),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct InstallSpecKey {
    key: String,
    message: String,
}

impl InstallSpecKey {
    pub fn new(unknown_key: &str) -> Self {
        let mut message = format!("Unrecognized Install Spec key: {unknown_key}. ");
        let mut corpus = CorpusBuilder::new().finish();

        corpus.add_text("requirements");
        corpus.add_text("embedded");
        corpus.add_text("components");
        corpus.add_text("environment");

        match corpus.search(unknown_key, 0.6).first() {
            Some(s) => message.push_str(format!("(Did you mean: '{}'?)", s.text).as_str()),
            None => {
                message.push_str(format!("(No similar keys found for: {}.)", unknown_key).as_str())
            }
        };
        Self {
            key: std::mem::take(&mut unknown_key.to_string()),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct SourceSpecKey {
    key: String,
    message: String,
}

impl SourceSpecKey {
    pub fn new(unknown_key: &str) -> Self {
        let mut message = format!("Unrecognized Source Spec key: {unknown_key}. ");
        let mut corpus = CorpusBuilder::new().finish();

        corpus.add_text("path");
        corpus.add_text("git");
        corpus.add_text("script");
        corpus.add_text("tar");
        corpus.add_text("ref");
        corpus.add_text("depth");
        corpus.add_text("exclude");
        corpus.add_text("filter");
        corpus.add_text("subdir");

        match corpus.search(unknown_key, 0.6).first() {
            Some(s) => message.push_str(format!("(Did you mean: '{}'?)", s.text).as_str()),
            None => {
                message.push_str(format!("(No similar keys found for: {}.)", unknown_key).as_str())
            }
        };
        Self {
            key: std::mem::take(&mut unknown_key.to_string()),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct BuildSpecKey {
    key: String,
    message: String,
}

impl BuildSpecKey {
    pub fn new(unknown_key: &str) -> Self {
        let mut message = format!("Unrecognized Build Spec key: {unknown_key}. ");
        let mut corpus = CorpusBuilder::new().finish();

        corpus.add_text("script");
        corpus.add_text("options");
        corpus.add_text("variants");
        corpus.add_text("validation");

        match corpus.search(unknown_key, 0.6).first() {
            Some(s) => message.push_str(format!("(Did you mean: '{}'?)", s.text).as_str()),
            None => {
                message.push_str(format!("(No similar keys found for: {}.)", unknown_key).as_str())
            }
        };
        Self {
            key: std::mem::take(&mut unknown_key.to_string()),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct TestSpecKey {
    key: String,
    message: String,
}

impl TestSpecKey {
    pub fn new(unknown_key: &str) -> Self {
        let mut message = format!("Unrecognized Test Spec key: {unknown_key}. ");
        let mut corpus = CorpusBuilder::new().finish();

        corpus.add_text("stage");
        corpus.add_text("script");
        corpus.add_text("selectors");
        corpus.add_text("requirements");

        match corpus.search(unknown_key, 0.6).first() {
            Some(s) => message.push_str(format!("(Did you mean: '{}'?)", s.text).as_str()),
            None => {
                message.push_str(format!("(No similar keys found for: {}.)", unknown_key).as_str())
            }
        };
        Self {
            key: std::mem::take(&mut unknown_key.to_string()),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct MetaSpecKey {
    key: String,
    message: String,
}

impl MetaSpecKey {
    pub fn new(unknown_key: &str) -> Self {
        let mut message = format!("Unrecognized Meta Spec key: {unknown_key}. ");
        let mut corpus = CorpusBuilder::new().finish();

        corpus.add_text("description");
        corpus.add_text("homepage");
        corpus.add_text("license");
        corpus.add_text("labels");

        match corpus.search(unknown_key, 0.6).first() {
            Some(s) => message.push_str(format!("(Did you mean: '{}'?)", s.text).as_str()),
            None => {
                message.push_str(format!("(No similar keys found for: {}.)", unknown_key).as_str())
            }
        };
        Self {
            key: std::mem::take(&mut unknown_key.to_string()),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub struct LintedItem<T> {
    pub item: T,
    pub lints: Vec<LintMessage>,
}

pub trait Lints {
    fn lints(&mut self) -> Vec<LintMessage>;
}

impl<T, V> From<V> for LintedItem<T>
where
    V: Lints + Into<T>,
{
    fn from(mut value: V) -> Self {
        Self {
            lints: value.lints(),
            item: value.into(),
        }
    }
}

// impl<T, V, E> TryFrom<V> for LintedItem<T>
// where
//     V: Lints + TryInto<T>,
//     E: serde::de::Error,
// {
//     fn try_from(mut value: V) -> Result<Self, E> {
//         Ok(Self {
//             lints: value.lints(),
//             item: value.try_into(),
//         })
//     }
// }
