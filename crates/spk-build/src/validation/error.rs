// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use miette::Diagnostic;
use relative_path::RelativePathBuf;
use spfs::env::SPFS_DIR;
use spk_schema::{BuildIdent, Request};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by failed package validation rules.
#[derive(Diagnostic, Debug, Error, Clone, PartialEq, Eq)]
#[diagnostic(
    url(
        "https://spkenv.dev/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spk::generic"))
    )
)]
pub enum Error {
    #[error("Builds must install at least one file in spfs")]
    #[diagnostic(severity(warning), code(spk::build::validation::empty_package))]
    EmptyPackageDenied,
    #[error("This build was expected to install no files, but did")]
    #[diagnostic(severity(warning), code(spk::build::validation::empty_package))]
    EmptyPackageRequired,

    #[error(
        r#"Package must include a build requirement for {request}

    because it's being built against {required_by},
    but {problem}
"#
    )]
    #[diagnostic(severity(warning), code(spk::build::validation::inherited_requirement))]
    DownstreamBuildRequestRequired {
        /// The package that was in the build environment and created the need for this request
        required_by: BuildIdent,
        /// The minimum request that is required downstream
        request: Request,
        /// Additional reasoning why an existing request was not sufficient
        problem: String,
    },
    #[error("Package was expected not to include a build requirement for {request}")]
    #[diagnostic(
        severity(warning),
        code(spk::build::validation::inherited_requirement),
        help(
            "This would need to be explicitly enabled in the package spec, which might have additional details"
        )
    )]
    DownstreamBuildRequestDenied {
        /// The inherited request that should have been excluded
        request: Request,
    },

    #[error(
        r#"Package must include a runtime requirement for {request}

    because it's being built against {required_by},
    but {problem}
"#
    )]
    #[diagnostic(severity(warning), code(spk::build::validation::inherited_requirement))]
    DownstreamRuntimeRequestRequired {
        /// The package that was in the build environment and created the need for this request
        required_by: BuildIdent,
        /// The minimum request that is required downstream
        request: Request,
        /// Additional reasoning why an existing request was not sufficient
        problem: String,
    },
    #[error("Package was expected to not include a runtime requirement for {request}")]
    #[diagnostic(
        severity(warning),
        code(spk::build::validation::inherited_requirement),
        help(
            "This would need to be explicitly enabled in the package spec, which might have additional details"
        )
    )]
    DownstreamRuntimeRequestDenied {
        /// The inherited request that should have been excluded
        request: Request,
    },

    #[error(
        r#"Builds are expected to collect all installed files

    this one was not collected: {SPFS_DIR}{path}
"#
    )]
    #[diagnostic(severity(warning), code(spk::build::validation::collect_all_files))]
    CollectAllFilesRequired { path: RelativePathBuf },
    #[error("This build was expected to ignore some files, but did not")]
    #[diagnostic(
        severity(warning),
        code(spk::build::validation::collect_all_files),
        help(
            "This would need to be explicitly enabled in the package spec, which might have additional details"
        )
    )]
    CollectAllFilesDenied,

    #[error("Build was expected to alter files from {owner}, but didn't")]
    #[diagnostic(
        severity(warning),
        code(spk::build::validation::alter_existing_files),
        help(
            "This would need to be explicitly enabled in the package spec, which might have additional details"
        )
    )]
    AlterExistingFilesRequired { owner: String },
    #[error(
        r#"Build must not alter files from other packages.

    {action} {SPFS_DIR}{path}
    which is owned by {owner}
"#
    )]
    #[diagnostic(severity(warning), code(spk::build::validation::alter_existing_files))]
    AlterExistingFilesDenied {
        owner: BuildIdent,
        path: RelativePathBuf,
        /// eg: "removed", "changed"
        action: &'static str,
    },

    #[error("Build was expected to collect files from {owner}, but didn't")]
    #[diagnostic(
        severity(warning),
        code(spk::build::validation::collect_existing_files),
        help(
            "This would need to be explicitly enabled in the package spec, which might have additional details"
        )
    )]
    CollectExistingFilesRequired { owner: String },
    #[error(
        r#"Build must not collect files from other packages.

    included {SPFS_DIR}{path}
    which is owned by {owner}
"#
    )]
    #[diagnostic(
        severity(warning),
        code(spk::build::validation::collect_existing_files)
    )]
    CollectExistingFilesDenied {
        owner: BuildIdent,
        path: RelativePathBuf,
    },

    #[error("This package was found in its own build environment")]
    #[diagnostic(severity(warning), code(spk::build::validation::recursive_build))]
    RecursiveBuildDenied(spk_schema::foundation::name::PkgNameBuf),
    #[error("This package was required to be in its own build environment, but was not")]
    #[diagnostic(
        severity(warning),
        code(spk::build::validation::recursive_build),
        help(
            "This would need to be explicitly enabled in the package spec, which might have additional details"
        )
    )]
    RecursiveBuildRequired(spk_schema::foundation::name::PkgNameBuf),
    #[error("Description over character limit")]
    #[diagnostic(severity(warning), code(spk::build::validation::long_var_description))]
    LongVarDescriptionDenied,
    #[error("Longer description required")]
    #[diagnostic(severity(warning), code(spk::build::validation::long_var_description))]
    LongVarDescriptionRequired,
    #[error("Description required for strong inheritance vars")]
    #[diagnostic(
        severity(warning),
        code(spk::build::validation::strong_inheritance_var_description)
    )]
    StrongInheritanceVarDescriptionRequired,

    #[error("A valid SPDX license required, nothing specified")]
    #[diagnostic(severity(warning), code(spk::build::validation::spdx_license))]
    SpdxLicenseMissing,
    #[error("A valid SPDX license required, got {given:?}")]
    #[diagnostic(severity(warning), code(spk::build::validation::spdx_license))]
    SpdxLicenseInvalid { given: String },
    #[error("Package should not have a license specified")]
    #[diagnostic(severity(warning), code(spk::build::validation::spdx_license))]
    SpdxLicenseDenied,
}
