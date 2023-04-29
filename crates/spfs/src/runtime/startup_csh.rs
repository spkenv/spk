// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

pub fn source<T>(tmpdir: &Option<T>) -> String
where
    T: AsRef<str>,
{
    let tmpdir_replacement = tmpdir
        .as_ref()
        .map(|value| {
            format!(
                r#"# Re-assign $TMPDIR because this value is lost when
# exec'ing a privileged process.
setenv TMPDIR "{}"

"#,
                value.as_ref()
            )
        })
        .unwrap_or_default();

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

{tmpdir_replacement}
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
    $argv:q
    exit $?
endif

# csh cannot echo to stderr, only sh can do that :/
/bin/sh -c "echo '$SPFS_SHELL_MESSAGE' 1>&2"
"#
    )
}
