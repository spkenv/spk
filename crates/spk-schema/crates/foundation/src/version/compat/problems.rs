// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Types that represent different variations of compatibility problems within
//! the same incompatibility category, so they can have their own display
//! implementations and differing associated data.

use std::collections::BTreeSet;

use super::{CommaSeparated, IncompatibleReason, IsSameReasonAs};
use crate::ident_build::Build;
use crate::name::{OptNameBuf, PkgNameBuf, RepositoryNameBuf};
use crate::version::Version;
use crate::version_range::{
    DoubleEqualsVersion,
    DoubleNotEqualsVersion,
    EqualsVersion,
    NotEqualsVersion,
};

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum VersionForClause {
    #[strum(to_string = "for {0}")]
    CompatVersion(Version),
    #[strum(to_string = "for >= {0}")]
    GteVersion(Version),
    #[strum(to_string = "for < {0}")]
    LtVersion(Version),
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum VersionRangeProblem {
    #[strum(to_string = "version too high {0}")]
    TooHigh(VersionForClause),
    #[strum(to_string = "version too low {0}")]
    TooLow(VersionForClause),
    #[strum(to_string = "not {op} {bound} [too low]")]
    NotHighEnough { op: &'static str, bound: Version },
    #[strum(to_string = "not {op} {bound} [too high]")]
    NotLowEnough { op: &'static str, bound: Version },
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum VersionNotDifferentProblem {
    #[strum(to_string = "excluded [{version}]")]
    NotEqual { version: NotEqualsVersion },
    #[strum(to_string = "excluded precisely [{version}]")]
    NotPreciselyEqual { version: DoubleNotEqualsVersion },
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum VersionNotEqualProblem {
    #[strum(to_string = "{other_version} !! {this_version} [not ]")]
    PartsNotEqual {
        this_version: EqualsVersion,
        other_version: Version,
    },
    #[strum(to_string = "{other_version} !! {this_version} [not equal @ prerelease]")]
    PreNotEqual {
        this_version: EqualsVersion,
        other_version: Version,
    },
    #[strum(to_string = "{other_version} !! {this_version} [not equal @ postrelease]")]
    PostNotEqual {
        this_version: EqualsVersion,
        other_version: Version,
    },
    #[strum(to_string = "{other_version} !! {this_version} [not equal precisely]")]
    PartsNotEqualPrecisely {
        this_version: DoubleEqualsVersion,
        other_version: Version,
    },
    #[strum(to_string = "{other_version} !! {this_version} [not equal precisely @ prerelease]")]
    PreNotEqualPrecisely {
        this_version: DoubleEqualsVersion,
        other_version: Version,
    },
    #[strum(to_string = "{other_version} !! {this_version} [not equal precisely @ postrelease]")]
    PostNotEqualPrecisely {
        this_version: DoubleEqualsVersion,
        other_version: Version,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum RangeSupersetProblem {
    #[strum(to_string = "[case {case},{index}] {this_range} does not contain {other_range}")]
    ContainProblem {
        case: u8,
        index: usize,
        // XXX: Is there an alternative to creating a string here?
        this_range: String,
        other_range: String,
    },
    #[strum(to_string = "{this_range} has stronger compatibility requirements than {other_range}")]
    StrongerCompatibilityRequirements {
        // XXX: Is there an alternative to creating a string here?
        this_range: String,
        other_range: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum PackageNameProblem {
    #[strum(to_string = "different package name {self_name} != {other_name}")]
    PkgRequest {
        self_name: PkgNameBuf,
        other_name: PkgNameBuf,
    },
    #[strum(to_string = "version selectors are for different packages {self_name} != {other_name}")]
    VersionSelector {
        self_name: PkgNameBuf,
        other_name: PkgNameBuf,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum BuildIdProblem {
    #[strum(
        to_string = "package and request differ in builds: requested {requested:?}; got {self_build:?}"
    )]
    PkgRequest {
        self_build: Build,
        requested: Option<Build>,
    },
    #[strum(to_string = "incompatible builds: {self_ident} && {other_ident}")]
    VersionSelector {
        // XXX: Is there an alternative to creating a string here?
        self_ident: String,
        other_ident: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum PackageRepoProblem {
    #[strum(
        to_string = "package did not come from requested repo (it was embedded in {parent_ident})"
    )]
    EmbeddedInPackageFromWrongRepository { parent_ident: String },
    #[strum(to_string = "package did not come from requested repo (it comes from a spec)")]
    FromRecipeFromWrongRepository,
    #[strum(
        to_string = "package did not come from requested repo (it comes from an internal test setup)"
    )]
    InternalTest,
    #[strum(to_string = "package did not come from requested repo: {self_repo} != {their_repo}")]
    WrongSourceRepository {
        self_repo: RepositoryNameBuf,
        their_repo: RepositoryNameBuf,
    },
    #[strum(
        to_string = "incompatible request for package {pkg} from differing repos: {self_repo} != {their_repo}"
    )]
    Restrict {
        pkg: PkgNameBuf,
        self_repo: RepositoryNameBuf,
        their_repo: RepositoryNameBuf,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum VarRequestProblem {
    #[strum(to_string = "request specifies a different namespace [{self_var} != {other_var}]")]
    DifferentNamespace {
        // XXX: Is there an alternative to creating a string here?
        self_var: String,
        other_var: String,
    },
    #[strum(to_string = "requests require different values [{self_value} != {other_value}]")]
    DifferentValue {
        self_value: String,
        other_value: String,
    },
    #[strum(to_string = "request is for a different var altogether [{self_var} != {other_var}]")]
    DifferentVar {
        // XXX: Is there an alternative to creating a string here?
        self_var: String,
        other_var: String,
    },
    #[strum(to_string = "fromBuildEnv requests cannot be reasonable compared")]
    Incomparable,
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum InclusionPolicyProblem {
    #[strum(to_string = "prerelease policy {our_policy} is more inclusive than {other_policy}")]
    Prerelease {
        our_policy: String,
        other_policy: String,
    },
    #[strum(to_string = "inclusion policy {our_policy} is more inclusive than {other_policy}")]
    Standard {
        our_policy: String,
        other_policy: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum VarOptionProblem {
    #[strum(to_string = "incompatible option, wanted '{assigned}'; got '{value}'")]
    Incompatible { assigned: String, value: String },
    #[strum(
        to_string = "incompatible build option '{var_request}': '{exact}' != '{request_value}'"
    )]
    IncompatibleBuildOption {
        var_request: OptNameBuf,
        exact: String,
        request_value: String,
    },
    #[strum(
        to_string = "incompatible build option '{var_request}': '{base}' != '{request_value}' and '{request_value}' is not a valid version number"
    )]
    IncompatibleBuildOptionInvalidVersion {
        var_request: OptNameBuf,
        base: String,
        request_value: String,
    },
    #[strum(
        to_string = "incompatible build option '{var_request}': '{exact}' != '{request_value}' and {context}"
    )]
    IncompatibleBuildOptionWithContext {
        var_request: OptNameBuf,
        exact: String,
        request_value: String,
        context: Box<IncompatibleReason>,
    },
}

impl IsSameReasonAs for VarOptionProblem {
    fn is_same_reason_as(&self, other: &Self) -> bool {
        match (self, other) {
            (
                VarOptionProblem::Incompatible { value: a, .. },
                VarOptionProblem::Incompatible { value: b, .. },
            ) => a == b,
            (
                VarOptionProblem::IncompatibleBuildOption { var_request: a, .. },
                VarOptionProblem::IncompatibleBuildOption { var_request: b, .. },
            ) => a == b,
            (
                VarOptionProblem::IncompatibleBuildOptionInvalidVersion { var_request: a, .. },
                VarOptionProblem::IncompatibleBuildOptionInvalidVersion { var_request: b, .. },
            ) => a == b,
            (
                VarOptionProblem::IncompatibleBuildOptionWithContext {
                    var_request: a,
                    context: b,
                    ..
                },
                VarOptionProblem::IncompatibleBuildOptionWithContext {
                    var_request: c,
                    context: d,
                    ..
                },
            ) => a == c && b.is_same_reason_as(d),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum ComponentsMissingProblem {
    #[strum(to_string = "does not define requested components: [{missing}]; found [{available}]")]
    ComponentsNotDefined {
        missing: CommaSeparated<BTreeSet<String>>,
        available: CommaSeparated<BTreeSet<String>>,
    },
    #[strum(
        to_string = "resolved package {package} does not provide all required components: needed {needed}; have {have}"
    )]
    ComponentsNotProvided {
        package: PkgNameBuf,
        needed: CommaSeparated<BTreeSet<String>>,
        have: CommaSeparated<BTreeSet<String>>,
    },
    #[strum(
        to_string = "package {embedder} embeds {embedded} but does not provide all required components: needed {needed}; have {have}"
    )]
    EmbeddedComponentsNotProvided {
        embedder: PkgNameBuf,
        embedded: PkgNameBuf,
        needed: CommaSeparated<BTreeSet<String>>,
        have: CommaSeparated<BTreeSet<String>>,
    },
}

impl IsSameReasonAs for ComponentsMissingProblem {
    fn is_same_reason_as(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ComponentsMissingProblem::ComponentsNotDefined { missing: a, .. },
                ComponentsMissingProblem::ComponentsNotDefined { missing: b, .. },
            ) => a == b,
            (
                ComponentsMissingProblem::ComponentsNotProvided {
                    package: a,
                    needed: b,
                    ..
                },
                ComponentsMissingProblem::ComponentsNotProvided {
                    package: c,
                    needed: d,
                    ..
                },
            ) => a == c && b == d,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum ImpossibleRequestProblem {
    #[strum(
        to_string = "depends on {pkg} which generates an impossible request {combined_request}"
    )]
    Cached {
        pkg: String,
        combined_request: String,
    },
    #[strum(
        to_string = "depends on {pkg} which generates an impossible request {pkg},{unresolved_request} - {inner_reason}"
    )]
    Restrict {
        pkg: String,
        unresolved_request: String,
        inner_reason: Box<IncompatibleReason>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum ConflictingRequirementProblem {
    #[strum(
        to_string = "package {pkg} requirement conflicts with existing package in solve: {inner_reason}"
    )]
    ExistingPackage {
        pkg: PkgNameBuf,
        inner_reason: Box<IncompatibleReason>,
    },
    #[strum(to_string = "conflicting requirement: {0}")]
    PkgRequirement(Box<IncompatibleReason>),
}

impl IsSameReasonAs for ConflictingRequirementProblem {
    fn is_same_reason_as(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ConflictingRequirementProblem::ExistingPackage {
                    pkg: a,
                    inner_reason: b,
                },
                ConflictingRequirementProblem::ExistingPackage {
                    pkg: c,
                    inner_reason: d,
                },
            ) => a == c && b.is_same_reason_as(d),
            (
                ConflictingRequirementProblem::PkgRequirement(a),
                ConflictingRequirementProblem::PkgRequirement(b),
            ) => a.is_same_reason_as(b),
            _ => false,
        }
    }
}
