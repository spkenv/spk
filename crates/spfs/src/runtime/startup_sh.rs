// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use itertools::Itertools;

use super::EnvKeyValue;

pub fn source(environment_overrides: &[EnvKeyValue]) -> String {
    let mut env_replacement = String::new();
    for (position, key_value) in environment_overrides.iter().with_position() {
        match position {
            itertools::Position::First | itertools::Position::Only => {
                env_replacement.push_str("# Re-assign variables as configured.\n");
                env_replacement.push_str("# The values of these variables may be lost when exec'ing a privileged process or unsharing the mount namespace.\n");
            }
            _ => {}
        };
        let value = key_value.1.replace("\"", "\\\"");
        env_replacement.push_str(&format!("export {key}=\"{value}\"\n", key = key_value.0));
        match position {
            itertools::Position::Last | itertools::Position::Only => {
                env_replacement.push('\n');
            }
            _ => {}
        };
    }

    format!(
        r#"#!/usr/bin/env sh
if [ -f ~/.bashrc ]; then
    source ~/.bashrc || true
fi

{env_replacement}
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
