// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::name::{PkgName, PkgNameBuf};

#[cfg(test)]
#[path = "./validation_test.rs"]
mod validation_test;

/// A Validator validates packages after they have been built
///
/// This type has been deprecated in favor of the more extensible
/// and configurable [`ValidationRule`] type, but remains in place
/// so that older specs can still be read.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[allow(clippy::enum_variant_names)]
pub enum LegacyValidator {
    MustInstallSomething,
    MustNotAlterExistingFiles,
    MustCollectAllFiles,
}

/// ValidationSpec configures how builds of this package
/// should be validated. The default spec contains all
/// recommended validators
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ValidationSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rules: Vec<ValidationRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled: Vec<LegacyValidator>,
}

impl ValidationSpec {
    pub fn is_default(&self) -> bool {
        self.rules.is_empty() && self.disabled.is_empty()
    }

    /// The rules as specified in the spec file. Usually this is not
    /// what you want, see [`Self::to_expanded_rules`].
    ///
    /// # Safety
    /// The unexpanded rules are as-specified in the spec file but do not
    /// include any required default rules or expanded rules. It also
    /// may have multiple conflicting rules that need to be further accounted
    /// for when determining actual validation criteria
    pub unsafe fn unexpanded_rules(&self) -> &Vec<ValidationRule> {
        &self.rules
    }

    /// Compute the final set of validation rules for this package
    ///
    /// This includes any default and implicit rules in the correct
    /// override order.
    pub fn to_expanded_rules(&self) -> Vec<ValidationRule> {
        let defaults = Self::default_rules()
            .into_iter()
            .flat_map(ValidationRule::with_implicit_additions)
            .collect::<Vec<_>>();
        let mut expanded = defaults;
        for rule in self.rules.iter().cloned() {
            let implicit_additions = rule.with_implicit_additions();
            expanded.extend(implicit_additions);
        }
        expanded
    }

    /// The default rules assumed for all packages
    pub fn default_rules() -> Vec<ValidationRule> {
        vec![
            ValidationRule::Deny {
                condition: ValidationMatcher::EmptyPackage,
            },
            ValidationRule::Deny {
                condition: ValidationMatcher::RecursiveBuild,
            },
            ValidationRule::Require {
                condition: ValidationMatcher::StrongInheritanceVarDescription,
            },
            ValidationRule::Deny {
                condition: ValidationMatcher::AlterExistingFiles {
                    packages: Vec::new(),
                    action: None,
                },
            },
            ValidationRule::Deny {
                condition: ValidationMatcher::CollectExistingFiles {
                    packages: Vec::new(),
                },
            },
            ValidationRule::Deny {
                condition: ValidationMatcher::LongVarDescription,
            },
            ValidationRule::Require {
                condition: ValidationMatcher::InheritRequirements {
                    packages: Vec::new(),
                },
            },
        ]
    }
}

/// Specifies an additional set of validation criteria for a package
///
/// These rules are meant to be evaluated in order with later rules
/// taking precedence over earlier ones with the same level of specificity
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, strum::EnumDiscriminants)]
#[strum_discriminants(derive(strum::EnumVariantNames, Serialize, Deserialize))]
#[strum_discriminants(serde(rename_all = "lowercase"))]
#[strum_discriminants(strum(serialize_all = "lowercase"))]
pub enum ValidationRule {
    Allow { condition: ValidationMatcher },
    Deny { condition: ValidationMatcher },
    Require { condition: ValidationMatcher },
}

impl ValidationRule {
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    pub fn is_deny(&self) -> bool {
        matches!(self, Self::Deny { .. })
    }

    pub fn is_require(&self) -> bool {
        matches!(self, Self::Require { .. })
    }

    /// The internal condition that is allowed, denied, required by this rule
    pub fn condition(&self) -> &ValidationMatcher {
        match self {
            Self::Allow { condition } => condition,
            Self::Deny { condition } => condition,
            Self::Require { condition } => condition,
        }
    }

    /// Create a new rule with the same allow/deny/require but an alternative condition
    pub fn with_condition(&self, condition: ValidationMatcher) -> Self {
        match self {
            ValidationRule::Allow { condition: _ } => Self::Allow { condition },
            ValidationRule::Deny { condition: _ } => Self::Deny { condition },
            ValidationRule::Require { condition: _ } => Self::Require { condition },
        }
    }

    /// Expands this rule, adding rules for any implicit ones that
    /// are defined as being a part of the current one.
    pub fn with_implicit_additions(self) -> Vec<ValidationRule> {
        match self.condition() {
            ValidationMatcher::RecursiveBuild => {
                // allowing a recursive build also implicitly assumes that
                // all files from the previous version of the package are okay
                // to modify, remove and collect into the new version
                vec![
                    self.with_condition(ValidationMatcher::AlterExistingFiles {
                        packages: vec![NameOrCurrent::Current],
                        action: Some(FileAlteration::Remove),
                    }),
                    self.with_condition(ValidationMatcher::AlterExistingFiles {
                        packages: vec![NameOrCurrent::Current],
                        action: Some(FileAlteration::Change),
                    }),
                    self.with_condition(ValidationMatcher::AlterExistingFiles {
                        packages: vec![NameOrCurrent::Current],
                        action: Some(FileAlteration::Touch),
                    }),
                    self.with_condition(ValidationMatcher::CollectExistingFiles {
                        packages: vec![NameOrCurrent::Current],
                    }),
                    self,
                ]
            }
            ValidationMatcher::AlterExistingFiles {
                packages,
                action: None,
            } => vec![
                self.with_condition(ValidationMatcher::AlterExistingFiles {
                    packages: packages.clone(),
                    action: Some(FileAlteration::Change),
                }),
                self.with_condition(ValidationMatcher::AlterExistingFiles {
                    packages: packages.clone(),
                    action: Some(FileAlteration::Remove),
                }),
            ],
            _ => vec![self],
        }
    }
}

#[derive(
    Debug,
    Clone,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Deserialize,
    Serialize,
    strum::EnumDiscriminants,
    strum::EnumVariantNames,
)]
#[strum_discriminants(derive(
    Hash,
    PartialOrd,
    Ord,
    strum::EnumVariantNames,
    Deserialize,
    Serialize
))]
pub enum ValidationMatcher {
    EmptyPackage,
    CollectAllFiles,
    StrongInheritanceVarDescription,
    LongVarDescription,
    AlterExistingFiles {
        packages: Vec<NameOrCurrent>,
        action: Option<FileAlteration>,
    },
    CollectExistingFiles {
        packages: Vec<NameOrCurrent>,
    },
    RecursiveBuild,
    InheritRequirements {
        packages: Vec<PkgNameBuf>,
    },
}

#[derive(
    Debug, Default, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub enum FileAlteration {
    #[default]
    Change,
    Remove,
    Touch,
}

/// Either a package name or a special reference to the current package
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum NameOrCurrent {
    #[serde(rename = "Self")]
    Current,
    Name(PkgNameBuf),
}

impl NameOrCurrent {
    /// Returns the contained package name or the
    /// provided current package as appropriate
    pub fn or_current<'a, 'b: 'a>(&'a self, current: &'b PkgName) -> &'a PkgName {
        match self {
            Self::Name(n) => n,
            Self::Current => current,
        }
    }

    /// The named package, if this is not [`Self::Current`]
    pub fn as_name(&self) -> Option<&PkgName> {
        match self {
            Self::Current => None,
            Self::Name(n) => Some(n.as_ref()),
        }
    }
}

impl From<PkgNameBuf> for NameOrCurrent {
    fn from(value: PkgNameBuf) -> Self {
        Self::Name(value)
    }
}

impl<'de> Deserialize<'de> for ValidationRule {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ValidationRuleVisitor;

        impl<'de> serde::de::Visitor<'de> for ValidationRuleVisitor {
            type Value = ValidationRule;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an allow, deny, or require rule")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                use ValidationRuleDiscriminants as Kind;
                let kind = map
                    .next_key::<Kind>()?
                    .ok_or_else(|| serde::de::Error::missing_field("allow, deny, or require"))?;
                match kind {
                    Kind::Allow => Ok(ValidationRule::Allow {
                        condition: Self::deserialize_partial_matcher(map)?,
                    }),
                    Kind::Deny => Ok(ValidationRule::Deny {
                        condition: Self::deserialize_partial_matcher(map)?,
                    }),
                    Kind::Require => Ok(ValidationRule::Require {
                        condition: Self::deserialize_partial_matcher(map)?,
                    }),
                }
            }
        }

        impl<'de> ValidationRuleVisitor {
            fn deserialize_partial_matcher<A>(mut map: A) -> Result<ValidationMatcher, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                use ValidationMatcherDiscriminants as Kind;
                let kind = map.next_value::<Kind>()?;
                match kind {
                    Kind::EmptyPackage => Ok(ValidationMatcher::EmptyPackage),
                    Kind::CollectAllFiles => Ok(ValidationMatcher::EmptyPackage),
                    Kind::StrongInheritanceVarDescription => {
                        Ok(ValidationMatcher::StrongInheritanceVarDescription)
                    }
                    Kind::LongVarDescription => Ok(ValidationMatcher::LongVarDescription),
                    Kind::AlterExistingFiles => {
                        let mut packages = Default::default();
                        let mut action = None;
                        while let Some(name) = map.next_key::<String>()? {
                            match name.as_str() {
                                "packages" => packages = map.next_value()?,
                                "action" => action = map.next_value()?,
                                unknown => {
                                    return Err(serde::de::Error::unknown_field(
                                        unknown,
                                        &["packages", "action"],
                                    ));
                                }
                            }
                        }
                        Ok(ValidationMatcher::AlterExistingFiles { packages, action })
                    }
                    Kind::InheritRequirements => {
                        let packages = if let Some((name, value)) =
                            map.next_entry::<String, Vec<PkgNameBuf>>()?
                        {
                            if name != "packages" {
                                return Err(serde::de::Error::unknown_field(&name, &["packages"]));
                            }
                            value
                        } else {
                            Vec::new()
                        };
                        Ok(ValidationMatcher::InheritRequirements { packages })
                    }
                    Kind::CollectExistingFiles => {
                        let packages = if let Some((name, value)) =
                            map.next_entry::<String, Vec<NameOrCurrent>>()?
                        {
                            if name != "packages" {
                                return Err(serde::de::Error::unknown_field(&name, &["packages"]));
                            }
                            value
                        } else {
                            Vec::new()
                        };
                        Ok(ValidationMatcher::CollectExistingFiles { packages })
                    }
                    Kind::RecursiveBuild => Ok(ValidationMatcher::RecursiveBuild),
                }
            }
        }

        deserializer.deserialize_map(ValidationRuleVisitor)
    }
}

impl Serialize for ValidationRule {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use ValidationMatcherDiscriminants as Kind;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_key(&ValidationRuleDiscriminants::from(self))?;
        let condition = self.condition();
        map.serialize_value(&Kind::from(condition))?;
        match condition {
            ValidationMatcher::RecursiveBuild
            | ValidationMatcher::CollectAllFiles
            | ValidationMatcher::StrongInheritanceVarDescription
            | ValidationMatcher::LongVarDescription
            | ValidationMatcher::EmptyPackage => {}
            ValidationMatcher::InheritRequirements { packages } => {
                if !packages.is_empty() {
                    map.serialize_entry("packages", packages)?;
                }
            }
            ValidationMatcher::AlterExistingFiles { packages, action } => {
                if !packages.is_empty() {
                    map.serialize_entry("packages", packages)?;
                }
                if let Some(action) = action {
                    map.serialize_entry("action", action)?;
                }
            }
            ValidationMatcher::CollectExistingFiles { packages } => {
                if !packages.is_empty() {
                    map.serialize_entry("packages", packages)?;
                }
            }
        }
        map.end()
    }
}
