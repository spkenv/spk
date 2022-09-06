// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::parse_modinfo_params;

#[rstest]
#[cfg(target_os = "linux")]
fn test_parse_modinfo() {
    const MODINFO: &str = r#"
filename:       /lib/modules/3.10.0-1160.71.1.el7.x86_64/kernel/fs/overlayfs/overlay.ko.xz
alias:          fs-overlay
license:        GPL
description:    Overlay filesystem
author:         Miklos Szeredi <miklos@szeredi.hu>
retpoline:      Y
rhelversion:    7.9
srcversion:     35816F78BA4302A3CFE3853
depends:
intree:         Y
vermagic:       3.10.0-1160.71.1.el7.x86_64 SMP mod_unload modversions
signer:         CentOS Linux kernel signing key
sig_key:        6D:A7:C2:41:B1:C9:99:25:3F:B3:B0:36:89:C0:D1:E3:BE:27:82:E4
sig_hashalgo:   sha256
parm:           check_copy_up:uint
parm:           ovl_check_copy_up:Warn on copy-up when causing process also has a R/O fd open
parm:           redirect_max:ushort
parm:           ovl_redirect_max:Maximum length of absolute redirect xattr value
parm:           redirect_dir:bool
parm:           ovl_redirect_dir_def:Default to on or off for the redirect_dir feature
parm:           redirect_always_follow:bool
parm:           ovl_redirect_always_follow:Follow redirects even if redirect_dir feature is turned off
parm:           index:bool
parm:           ovl_index_def:Default to on or off for the inodes index feature
parm:           nfs_export:bool
parm:           ovl_nfs_export_def:Default to on or off for the NFS export feature
parm:           xino_auto:bool
parm:           ovl_xino_auto_def:Auto enable xino feature
"#;
    let params = parse_modinfo_params(&mut std::io::BufReader::new(std::io::Cursor::new(MODINFO)))
        .expect("modinfo parsing should not fail");
    let mut params = params.iter().map(String::as_str).collect::<Vec<_>>();
    params.sort();
    assert_eq!(
        params,
        vec![
            "check_copy_up",
            "index",
            "nfs_export",
            "ovl_check_copy_up",
            "ovl_index_def",
            "ovl_nfs_export_def",
            "ovl_redirect_always_follow",
            "ovl_redirect_dir_def",
            "ovl_redirect_max",
            "ovl_xino_auto_def",
            "redirect_always_follow",
            "redirect_dir",
            "redirect_max",
            "xino_auto",
        ]
    );
}
