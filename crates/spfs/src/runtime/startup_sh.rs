// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

pub static SOURCE: &str = r#"#!/usr/bin/env sh
if [[ -f ~/.bashrc ]]; then
    source ~/.bashrc || true
fi
startup_dir="/spfs/etc/spfs/startup.d"
if [[ -d ${startup_dir} ]]; then
    filenames=$(/bin/ls $startup_dir | grep '\.sh$')
    if [[ ! -z "$filenames" ]]; then
        for file in $filenames; do
            [[ -z "$SPFS_DEBUG" ]] || echo source $startup_dir/$file 1>&2
            source $startup_dir/$file || true
        done
    fi
fi

if [[ "$#" -ne 0 ]]; then
    "$@"
    exit $?
fi

echo "* You are now in an configured subshell *" 1>&2
"#;
