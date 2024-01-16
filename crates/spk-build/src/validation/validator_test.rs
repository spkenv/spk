// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use itertools::Itertools;
use relative_path::RelativePathBuf;
use spk_schema::foundation::fixtures::*;
use spk_schema::validation::ValidationMatcherDiscriminants;
use spk_schema::BuildIdent;

use super::{Error, Outcome, Report, Status, Subject};

#[tokio::test]
async fn test_validate_rule_layering_locality() {
    init_logging();

    // test that the specificity of each rule is taken into consideration
    // eg: when files are changed from dependencies, but there are different
    // rules for global and package-level validations we want the most specific
    // to win, or later in the case when they are the same

    let do_not_change: BuildIdent = "do-not-change/1.0.0/3I42H3S6".parse().unwrap();
    let do_not_change_file = RelativePathBuf::from("/do_not_change_file");
    let do_change: BuildIdent = "do-change/1.0.0/3I42H3S6".parse().unwrap();
    let do_change_file = RelativePathBuf::from("/do_change_file");
    let denied_do_change = || Error::AlterExistingFilesDenied {
        owner: do_change.clone(),
        path: do_change_file.clone(),
        action: "changed",
    };
    let denied_do_not_change = || Error::AlterExistingFilesDenied {
        owner: do_not_change.clone(),
        path: do_not_change_file.clone(),
        action: "changed",
    };

    let outcomes = vec![
        // starting with two failed checks for altered files, they each
        // identify a specific file that should not have been changed
        // and represent the outcomes of the default rule to deny all
        // file alterations
        Outcome {
            condition: ValidationMatcherDiscriminants::AlterExistingFiles,
            locality: String::new(),
            subject: Subject::Path(do_change.clone(), do_change_file.clone()),
            status: Status::Denied(denied_do_change()),
        },
        Outcome {
            condition: ValidationMatcherDiscriminants::AlterExistingFiles,
            locality: String::new(),
            subject: Subject::Path(do_not_change.clone(), do_not_change_file.clone()),
            status: Status::Denied(denied_do_not_change()),
        },
        // followed by the outcome of a rule to allow alterations of a specific package
        Outcome {
            condition: ValidationMatcherDiscriminants::AlterExistingFiles,
            locality: do_change.name().to_string(),
            subject: Subject::Path(do_change.clone(), do_change_file.clone()),
            status: Status::Allowed,
        },
    ];

    // The specificity of the outcomes above should ensure that the order of the
    // outcomes does not affect the final result of the report.
    let count = outcomes.len();
    for shuffled in outcomes.into_iter().permutations(count) {
        let errors = Report::from_iter(shuffled).into_errors();
        assert_eq!(errors.len(), 1, "Only one denied error should remain");
        assert_eq!(
            errors[0],
            denied_do_not_change(),
            "The remaining error should be for do_not_change"
        )
    }
}

#[tokio::test]
async fn test_validate_rule_recursive() {
    init_logging();

    // test that the specificity of each rule is taken into consideration
    // eg: when files are changed from dependencies, but there are different
    // rules for global and package-level validations we want the most specific
    // to win, or later in the case when they are the same

    let circ_1 = || -> BuildIdent { "circ/1.0.0/3I42H3S6".parse().unwrap() };
    let version_path = || RelativePathBuf::from("/version.txt");

    // these were pulled from the state of the recursive build tests
    // in the spk-cmd-build crate (at the time of writing)
    let outcomes = vec![
        // initially, the default rules from the package spec deny
        // the alteration of files from the build environment as well
        // as the collection of files from other packages and the recursive
        // build itself
        Outcome {
            condition: ValidationMatcherDiscriminants::AlterExistingFiles,
            locality: "Change/".into(),
            subject: Subject::Path(circ_1(), version_path()),
            status: Status::Denied(Error::AlterExistingFilesDenied {
                owner: circ_1(),
                path: version_path(),
                action: "changed",
            }),
        },
        Outcome {
            condition: ValidationMatcherDiscriminants::CollectExistingFiles,
            locality: "".into(),
            subject: Subject::Path(circ_1(), version_path()),
            status: Status::Denied(Error::CollectExistingFilesDenied {
                owner: circ_1(),
                path: version_path(),
            }),
        },
        Outcome {
            condition: ValidationMatcherDiscriminants::RecursiveBuild,
            locality: "".into(),
            subject: Subject::Everything,
            status: Status::Denied(Error::RecursiveBuildDenied(circ_1().name().to_owned())),
        },
        Outcome {
            condition: ValidationMatcherDiscriminants::AlterExistingFiles,
            locality: "Change/".into(),
            subject: Subject::Path(circ_1(), version_path()),
            status: Status::Denied(Error::AlterExistingFilesDenied {
                owner: circ_1(),
                path: version_path(),
                action: "changed",
            }),
        },
        Outcome {
            condition: ValidationMatcherDiscriminants::CollectExistingFiles,
            locality: "".into(),
            subject: Subject::Path(circ_1(), version_path()),
            status: Status::Denied(Error::CollectExistingFilesDenied {
                owner: circ_1(),
                path: version_path(),
            }),
        },
        // the implicit recursive build rules then appear, and allow
        // the previously denied actions within the locality of the
        // current package being built
        Outcome {
            condition: ValidationMatcherDiscriminants::AlterExistingFiles,
            locality: "Change/circ".into(),
            subject: Subject::Everything,
            status: Status::Allowed,
        },
        Outcome {
            condition: ValidationMatcherDiscriminants::AlterExistingFiles,
            locality: "Remove/circ".into(),
            subject: Subject::Everything,
            status: Status::Allowed,
        },
        Outcome {
            condition: ValidationMatcherDiscriminants::AlterExistingFiles,
            locality: "Touch/circ".into(),
            subject: Subject::Everything,
            status: Status::Allowed,
        },
        Outcome {
            condition: ValidationMatcherDiscriminants::CollectExistingFiles,
            locality: "circ".into(),
            subject: Subject::Everything,
            status: Status::Allowed,
        },
        Outcome {
            condition: ValidationMatcherDiscriminants::RecursiveBuild,
            locality: "".into(),
            subject: Subject::Everything,
            status: Status::Allowed,
        },
    ];

    Report::from_iter(outcomes)
        .into_result()
        .expect("Recursive build should be allowed");
}
