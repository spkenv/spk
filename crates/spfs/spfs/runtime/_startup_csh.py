source = """#!/usr/bin/env csh
if ( -f ~/.tcshrc ) then
    source ~/.tcshrc || true
else if ( -f ~/.cshrc ) then
    source ~/.cshrc || true
endif

set startup_dir="/spfs/etc/spfs/startup.d"
if ( -d "${startup_dir}" != 0 ) then
    set filenames=`/bin/ls $startup_dir | grep '\.csh\s*$'`
    if ( "$filenames" != "" ) then
        foreach file ($filenames)
            if ( $?SPFS_DEBUG ) then
                # csh cannot echo to stderr, only sh can do that :/
                /bin/sh -c "echo source ${startup_dir}/$file 1>&2"
            endif
            source ${startup_dir}/$file || true
        end
    endif
endif

if ( "$#argv" != 0 ) then
    $argv:q
    exit $?
endif

# csh cannot echo to stderr, only sh can do that :/
/bin/sh -c "echo '* You are now in an spfs-configured shell *' 1>&2"
"""
