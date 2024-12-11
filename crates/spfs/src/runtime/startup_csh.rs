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
        env_replacement.push_str(&format!("setenv {key} \"{value}\"\n", key = key_value.0));
        match position {
            itertools::Position::Last | itertools::Position::Only => {
                env_replacement.push('\n');
            }
            _ => {}
        };
    }

    format!(
        r#"#!/usr/bin/env csh
if ($?SPFS_ORIGINAL_HOME) then
    setenv HOME "$SPFS_ORIGINAL_HOME"
    unsetenv SPFS_ORIGINAL_HOME
endif
if ( -f ~/.tcshrc ) then
    source ~/.tcshrc || true
else if ( -f ~/.cshrc ) then
    source ~/.cshrc || true
endif

{env_replacement}
set startup_dir="/spfs/etc/spfs/startup.d"
if ( -d "${{startup_dir}}" != 0 ) then
    set filenames=`/bin/ls $startup_dir | grep '\.csh\s*$'`
    if ( "$filenames" != "" ) then
        foreach file ($filenames)
            if ( $?SPFS_DEBUG ) then
                # csh cannot echo to stderr, only sh can do that :/
                /bin/sh -c "echo source ${{startup_dir}}/$file 1>&2"
            endif
            source ${{startup_dir}}/$file || true
        end
    endif
endif

if ( "$#argv" != 0 ) then
    exec $argv:q
endif

if ("$SPFS_SHELL_MESSAGE" != "") then
    # csh cannot echo to stderr, only sh can do that :/
    /bin/sh -c "echo '$SPFS_SHELL_MESSAGE' 1>&2"
endif
"#
    )
}
