// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use crate::{Error, OsError};

/// Recover a failed rename only when the destination's complete encoding matches.
#[rstest]
#[case::matching_destination(Some(b"expected".as_slice()), true)]
#[case::same_length_mismatch(Some(b"modified".as_slice()), false)]
#[case::truncated_destination(Some(b"exp".as_slice()), false)]
#[case::missing_destination(None, false)]
#[tokio::test]
async fn test_rename_object_recovery(#[case] existing: Option<&[u8]>, #[case] succeeds: bool) {
    let tmpdir = tempfile::tempdir().unwrap();
    let source_path = tmpdir.path().join("missing-source");
    let destination_path = tmpdir.path().join("destination");
    if let Some(existing) = existing {
        std::fs::write(&destination_path, existing).unwrap();
    }
    let result = super::rename_object(&source_path, &destination_path, b"expected").await;
    if succeeds {
        result.unwrap();
    } else {
        let err = result.unwrap_err();
        let message = err.to_string();
        assert!(message.contains("destination validation:"));
        assert!(message.contains(if existing.is_some() {
            "encoded contents differ"
        } else {
            "read failed"
        }));
        let Error::StorageObjectRenameError { source, .. } = err else {
            panic!("expected original rename error");
        };
        assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
    }
    assert!(!source_path.exists());
    if let Some(existing) = existing {
        assert_eq!(std::fs::read(&destination_path).unwrap(), existing);
    } else {
        assert!(!destination_path.exists());
    }
}

/// Keep the rename failure when an existing destination cannot be read as a file.
#[tokio::test]
async fn test_rename_object_unreadable_destination() {
    let tmpdir = tempfile::tempdir().unwrap();
    let source_path = tmpdir.path().join("source");
    let destination_path = tmpdir.path().join("destination");
    std::fs::write(&source_path, b"expected").unwrap();
    std::fs::create_dir(&destination_path).unwrap();
    let err = super::rename_object(&source_path, &destination_path, b"expected")
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("destination validation: read failed")
    );
    assert!(!source_path.exists());
    assert!(destination_path.is_dir());
}

/// A successful rename still publishes the source and removes its temporary name.
#[tokio::test]
async fn test_rename_object_success() {
    let tmpdir = tempfile::tempdir().unwrap();
    let source_path = tmpdir.path().join("source");
    let destination_path = tmpdir.path().join("destination");
    std::fs::write(&source_path, b"expected").unwrap();
    super::rename_object(&source_path, &destination_path, b"expected")
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination_path).unwrap(), b"expected");
    assert!(!source_path.exists());
}

/// Distinguish missing sources, missing destination parents, and existing targets.
#[rstest]
#[case::missing_source(false, true, false)]
#[case::missing_destination_parent(true, false, false)]
#[case::existing_destination(false, true, true)]
#[tokio::test]
async fn test_object_rename_error(
    #[case] source_exists: bool,
    #[case] destination_parent_exists: bool,
    #[case] destination_exists: bool,
) {
    let tmpdir = tempfile::tempdir().unwrap();
    let source_path = tmpdir.path().join("work").join("temporary-object");
    let destination_path = tmpdir.path().join("objects").join("digest");
    std::fs::create_dir(source_path.parent().unwrap()).unwrap();
    if source_exists {
        std::fs::write(&source_path, b"object").unwrap();
    }
    if destination_parent_exists {
        std::fs::create_dir(destination_path.parent().unwrap()).unwrap();
    }
    if destination_exists {
        std::fs::write(&destination_path, b"existing").unwrap();
    }

    let source = tokio::fs::rename(&source_path, &destination_path)
        .await
        .unwrap_err();
    let errno = source.raw_os_error();
    let kind = source.kind();
    let original_message = source.to_string();
    let err = super::object_rename_error(&source_path, &destination_path, source).await;
    assert_eq!(err.os_error(), errno);
    let message = err.to_string();
    assert!(message.contains(&original_message));
    assert!(message.contains("not an atomic snapshot"));
    let Error::StorageObjectRenameError {
        source_path: reported_source,
        destination_path: reported_destination,
        observations,
        source,
    } = err
    else {
        panic!("expected an object rename error");
    };
    assert_eq!(reported_source, source_path);
    assert_eq!(reported_destination, destination_path);
    assert_eq!(source.kind(), kind);
    for (label, path, expected_status) in [
        (
            "source",
            source_path.as_path(),
            if source_exists {
                "file (6 bytes)"
            } else {
                "metadata error:"
            },
        ),
        (
            "destination",
            destination_path.as_path(),
            if destination_exists {
                "file (8 bytes)"
            } else {
                "metadata error:"
            },
        ),
        ("source parent", source_path.parent().unwrap(), "directory"),
        (
            "destination parent",
            destination_path.parent().unwrap(),
            if destination_parent_exists {
                "directory"
            } else {
                "metadata error:"
            },
        ),
    ] {
        assert!(
            observations.contains(&format!("{label}: {}: {expected_status}", path.display())),
            "{observations}"
        );
    }
    assert_eq!(source_path.exists(), source_exists);
    assert_eq!(destination_path.exists(), destination_exists);
}
