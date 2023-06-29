// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

pub fn source<T>(tmpdir: Option<&T>) -> String
where
    T: AsRef<str>,
{
    let tmpdir_replacement = tmpdir
        .as_ref()
        .map(|value| {
            format!(
                r#"# Re-assign $TMPDIR because this value is lost when
# exec'ing a privileged process.
export TMPDIR="{}"

"#,
                value.as_ref()
            )
        })
        .unwrap_or_default();

    format!(
        r#"#!/usr/bin/env sh
if [ -f ~/.bashrc ]; then
    source ~/.bashrc || true
fi

{tmpdir_replacement}
startup_dir="/spfs/etc/spfs/startup.d"
if [ -d "${{startup_dir}}" ]; then
    filenames=$(/bin/ls $startup_dir | grep '\.sh$')
    if [ ! -z "$filenames" ]; then
        for file in $filenames; do
            [ -z "$SPFS_DEBUG" ] || echo source $startup_dir/$file 1>&2
            . $startup_dir/$file || true
        done
    fi
fi

if [ "$#" -ne 0 ]; then
    exec "$@"
fi

if [ ! -z "$SPFS_SHELL_MESSAGE" ]; then
    echo "$SPFS_SHELL_MESSAGE" 1>&2
fi
"#
    )
}
