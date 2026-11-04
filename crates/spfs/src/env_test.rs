// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use super::*;

/// Namespace restrictions override configuration without changing host options.
#[rstest]
#[case::on((OverlayFsRedirectDir::On, "redirect_dir=on", true))]
#[case::follow((OverlayFsRedirectDir::Follow, "redirect_dir=follow", false))]
#[case::nofollow((OverlayFsRedirectDir::NoFollow, "redirect_dir=nofollow", false))]
#[case::off((OverlayFsRedirectDir::Off, "redirect_dir=off", false))]
fn test_overlay_mount_options(
    #[case] redirect: (OverlayFsRedirectDir, &'static str, bool),
    #[values(false, true)] user_xattr: bool,
    #[values(false, true)] read_only: bool,
    #[values(false, true)] break_hardlinks: bool,
    #[values(false, true)] metadata_copy_up: bool,
) {
    let (redirect_dir, redirect_option, allows_features) = redirect;
    let options = OverlayMountOptions {
        global_options: OverlayFsOptions {
            redirect_dir,
            break_hardlinks,
            metadata_copy_up,
        },
        read_only,
        lowerdir_append: false,
        user_xattr,
    };
    let index_on = !user_xattr && allows_features && !break_hardlinks;
    let metacopy_on = !user_xattr && allows_features && metadata_copy_up;
    assert_eq!(options.index_on(), index_on);
    assert_eq!(options.metacopy_on(), metacopy_on);

    let params = runtime::overlayfs::overlayfs_available_options();
    let mut expected = Vec::new();
    if read_only {
        expected.push("ro");
    }
    if user_xattr {
        expected.push("userxattr");
    }
    if params.contains("index") {
        expected.push(if index_on { "index=on" } else { "index=off" });
    }
    if params.contains("metacopy") {
        expected.push(if metacopy_on {
            "metacopy=on"
        } else {
            "metacopy=off"
        });
    }
    expected.push(if user_xattr {
        "redirect_dir=nofollow"
    } else {
        redirect_option
    });
    assert_eq!(options.to_options(), expected);
}
