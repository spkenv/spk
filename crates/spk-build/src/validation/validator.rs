// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;

use relative_path::RelativePathBuf;
use spk_schema::validation::{ValidationMatcher, ValidationMatcherDiscriminants};
use spk_schema::{BuildIdent, Package, Variant};

use super::Error;
use crate::report::{BuildReport, BuildSetupReport};

#[cfg(test)]
#[path = "./validator_test.rs"]
mod validator_test;

/// Validates some aspect of a package build
#[async_trait::async_trait]
pub trait Validator: sealed::Sealed {
    /// Check the initial build setup before actually
    /// running the build script
    async fn validate_setup<P, V>(&self, report: &BuildSetupReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync;

    /// Check the output and final status of the build
    async fn validate_build<P, V>(&self, report: &BuildReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        self.validate_setup(&report.setup).await
    }
}

impl sealed::Sealed for spk_schema::ValidationRule {}

macro_rules! rule_to_validator {
    ($rule:ident, $bind:ident, $op:tt) => {{
        let kind = spk_schema::validation::ValidationRuleDiscriminants::from($rule);
        #[allow(unused_braces)]
        match $rule.condition() {
            ValidationMatcher::SpdxLicense => {
                let $bind = super::SpdxLicenseValidator { kind };
                $op
            }
            ValidationMatcher::EmptyPackage => {
                let $bind = super::EmptyPackageValidator { kind };
                $op
            }
            ValidationMatcher::CollectAllFiles => {
                let $bind = super::CollectAllFilesValidator { kind };
                $op
            }
            ValidationMatcher::StrongInheritanceVarDescription => {
                let $bind = super::StrongInheritanceVarDescriptionValidator { kind };
                $op
            }
            ValidationMatcher::LongVarDescription => {
                let $bind = super::LongVarDescriptionValidator { kind };
                $op
            }
            ValidationMatcher::AlterExistingFiles { packages, action } => {
                let $bind = super::AlterExistingFilesValidator {
                    kind,
                    packages,
                    action: action.as_ref(),
                };
                $op
            }
            ValidationMatcher::CollectExistingFiles { packages } => {
                let $bind = super::CollectExistingFilesValidator { kind, packages };
                $op
            }
            ValidationMatcher::RecursiveBuild => {
                let $bind = super::RecursiveBuildValidator { kind };
                $op
            }
            ValidationMatcher::InheritRequirements { packages } => {
                let $bind = super::InheritRequirementsValidator { kind, packages };
                $op
            }
        }
    }};
}

#[async_trait::async_trait]
impl Validator for spk_schema::ValidationRule {
    async fn validate_setup<P, V>(&self, setup: &BuildSetupReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        rule_to_validator!(self, v, { v.validate_setup(setup).await })
    }

    async fn validate_build<P, V>(&self, report: &BuildReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        rule_to_validator!(self, v, { v.validate_build(report).await })
    }
}

/// Contains the results of all performed validation checks.
/// Manages the overriding and shadowing of past results as new
/// validation results are added, see [`Outcome`].
///
/// Many of these results may represent validation that passed. Use
/// [`Self::into_result`] to build an error from any failed results.
///
/// Reports should generally not be created empty when being returned
/// by a validator. An empty report has no rules to be applied and
/// cannot effectively merge or override previous results that it should.
#[derive(Debug)]
pub struct Report {
    by_kind: HashMap<ValidationMatcherDiscriminants, Vec<Outcome>>,
}

impl Report {
    /// Create a report that marks all paths as allowed for the identified condition.
    ///
    /// The locality of this condition is assumed to be nothing, meaning that
    /// any current or future results that has an associated locality will supersede
    /// this one, see [`Outcome`]
    pub fn entire_build_allowed(condition: ValidationMatcherDiscriminants) -> Self {
        Self::entire_build_allowed_at(condition, Option::<String>::None)
    }

    /// Same as [`Self::entire_build_allowed`] except that the result can be repeated
    /// for one or more localities, creating rules that would override one without a locality,
    /// see [`Outcome`]
    ///
    /// If `localities` is empty, a general result is still created making this is the same
    /// as calling [`Self::entire_build_allowed`].
    pub fn entire_build_allowed_at<I, S>(
        condition: ValidationMatcherDiscriminants,
        localities: I,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::by_localities(localities, |locality| Outcome {
            subject: Subject::Everything,
            condition,
            locality,
            status: Status::Allowed,
        })
    }

    /// Create a report that marks all paths as not matched for the identified condition.
    ///
    /// The locality of this condition is assumed to be nothing, meaning that
    /// any current or future results that has an associated locality will supersede
    /// this one, see [`Outcome`]
    pub fn entire_build_not_matched(condition: ValidationMatcherDiscriminants) -> Self {
        Self::entire_build_not_matched_at(condition, Option::<String>::None)
    }

    /// Same as [`Self::entire_build_not_matched`] except that the result can be repeated
    /// for one or more localities, creating rules that would override one without a locality,
    /// see [`Outcome`]
    ///
    /// If `localities` is empty, a general result is still created making this is the same
    /// as calling [`Self::entire_build_not_matched`].
    pub fn entire_build_not_matched_at<I, S>(
        condition: ValidationMatcherDiscriminants,
        localities: I,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::by_localities(localities, |locality| Outcome {
            subject: Subject::Everything,
            condition,
            locality,
            status: Status::NoMatch,
        })
    }

    /// Helper function to manage the creation of results for a number of
    /// localities. If the provided localities iterator is empty, will create
    /// one result with an empty locality to ensure a non-empty report.
    pub fn by_localities<I, S, F>(localities: I, mut for_each: F) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        F: FnMut(String) -> Outcome,
    {
        let mut report: Self = localities
            .into_iter()
            .map(Into::into)
            .map(&mut for_each)
            .collect();
        if report.by_kind.is_empty() {
            // if no localities were given, then create
            // one result with no locality as an empty
            // report would not function the same way
            report.add_outcome(for_each(String::new()));
        }
        report
    }

    /// Add an outcome to this report, handling any overrides and locality
    /// as described by [`Outcome`].
    ///
    /// Returns false if the rule was not inserted because another, more
    /// specific one already exists.
    pub fn add_outcome(&mut self, outcome: Outcome) -> bool {
        let results = self.by_kind.entry(outcome.condition).or_default();
        if results.iter().any(|r| outcome.is_silenced_by(r)) {
            return false;
        }
        results.retain(|r| !r.is_overridden_by(&outcome));
        results.push(outcome);
        true
    }

    /// Convert this report into a set of errors from the current state
    pub fn into_errors(self) -> Vec<Error> {
        self.by_kind
            .into_values()
            .flatten()
            .filter_map(|r| r.into_error())
            .collect()
    }

    /// Convert this report to a single result based on the current state
    pub fn into_result(self) -> crate::Result<()> {
        let validation_errors = self.into_errors();
        if validation_errors.is_empty() {
            Ok(())
        } else {
            Err(crate::Error::ValidationFailed {
                errors: validation_errors,
            })
        }
    }
}

impl From<Outcome> for Report {
    fn from(value: Outcome) -> Self {
        let mut new = Self {
            by_kind: Default::default(),
        };
        new.add_outcome(value);
        new
    }
}

impl FromIterator<Outcome> for Report {
    fn from_iter<T: IntoIterator<Item = Outcome>>(iter: T) -> Self {
        let mut new = Self {
            by_kind: Default::default(),
        };
        new.extend(iter);
        new
    }
}

impl FromIterator<Self> for Report {
    fn from_iter<T: IntoIterator<Item = Self>>(iter: T) -> Self {
        let mut new = Self {
            by_kind: Default::default(),
        };
        new.extend(iter);
        new
    }
}

impl Extend<Outcome> for Report {
    fn extend<T: IntoIterator<Item = Outcome>>(&mut self, iter: T) {
        for result in iter.into_iter() {
            self.add_outcome(result);
        }
    }
}

impl Extend<Self> for Report {
    fn extend<T: IntoIterator<Item = Self>>(&mut self, iter: T) {
        for report in iter.into_iter() {
            self.extend(report.by_kind.into_values().flatten());
        }
    }
}

/// Each validation rule operates against a location.
///
/// These are arbitrary hierarchies that determine which
/// combinations of allow/deny overrule one another. The
/// last, most specifically matched location for any one issue
/// is considered to be the result.
///
/// For example:
///   - deny: AlterExistingFiles
///   - allow: AlterExistingFiles
///     packages: ['python']
///
/// In the above set of rules, the first may deny altering a file
/// from the python package, but it would subsequently be allowed by
/// the next rule due to it being a more specific one. The first rule
/// might have the location of `AlterExistingFiles/` while the second
/// is `AlterExistingFiles/python` (more specific). A file altered from
/// outside the python package, however, would still be denied by the
/// the first
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The condition that was being looked for during validation
    pub condition: ValidationMatcherDiscriminants,
    /// Defines how specific this result is when considering overrides
    /// with other results (as described above).
    pub locality: String,
    pub subject: Subject,
    pub status: Status,
}

impl Outcome {
    /// True if the other, existing result would stop this one from
    /// having any effect. Unlike [`Self::is_overridden_by`],
    /// a result only silences another if it applies more broadly.
    pub fn is_silenced_by(&self, existing: &Self) -> bool {
        // in order to silence, the existing rule must override
        // this one but critically, it must also not have the
        // exact same locality since that would cause this rule
        // to also override it the other way
        self.is_overridden_by(existing) && existing.locality != self.locality
    }

    /// True if the other result would make this one obsolete
    pub fn is_overridden_by(&self, other: &Self) -> bool {
        if self.condition != other.condition {
            return false;
        }
        if !self.subject.is_overridden_by(&other.subject) {
            return false;
        }
        other.locality.starts_with(&self.locality)
    }

    /// Convert this result into an error, if appropriate
    pub fn into_error(self) -> Option<Error> {
        self.status.into_error()
    }
}

/// The specific part of the build environment being validated
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub enum Subject {
    /// Identifies all paths within the build environment, aka: the entire build
    Everything,
    /// The path to a specific file/folder within the spfs environment
    /// (and package that owns it)
    Path(BuildIdent, RelativePathBuf),
    /// All contents of the identified package within the build environment
    Package(BuildIdent),
}

impl Subject {
    pub fn is_everything(&self) -> bool {
        matches!(self, Self::Everything)
    }

    /// True if a result with this subject would be replaced by
    /// a result with the other subject. Ie the 'other' subject
    /// is considered to be the same or otherwise inclusive of this one
    pub fn is_overridden_by(&self, other: &Self) -> bool {
        other.contains(self)
    }

    /// True if the other subject is considered to be contained within this one
    pub fn contains(&self, other: &Self) -> bool {
        match (self, other) {
            // always overridden by another match of the same subject
            (a, b) if a == b => true,
            // the owner of two paths should be the same for the same environment
            // but just in case, we ignore this and consider the same paths always
            // equal regardless of if the validation tagged a different owner
            (Self::Path(_, a), Self::Path(_, b)) if a == b => true,
            // any "broader" subject would also override the other one, eg we match everything
            // and the other subject matches just a single path
            (Self::Everything, Self::Path(_, _)) | (Self::Everything, Self::Package(_)) => true,
            (Self::Package(a), Self::Path(b, _)) if a == b => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Status {
    /// The rule did not match and this was allowed
    NoMatch,
    /// The rule matched and this was allowed
    Allowed,
    /// The rule matched but was not allowed to
    Denied(Error),
    /// The rule did not match but was required to
    Required(Error),
}

impl Status {
    pub fn into_error(self) -> Option<Error> {
        match self {
            Status::NoMatch | Status::Allowed => None,
            Status::Denied(err) | Status::Required(err) => Some(err),
        }
    }
}

pub(crate) mod sealed {
    pub trait Sealed {}
}
