source = """#!/usr/bin/env csh
if ( -f ~/.tcshrc ) then
    source ~/.tcshrc
else if ( -f ~/.cshrc ) then
    source ~/.cshrc
endif

set startup_dir="/spfs/etc/spfs/startup.d"
if ( -d "${startup_dir}" != 0 ) then
    set filenames=`/bin/ls $startup_dir | grep '\.csh\s*$'`
    if ( "$filenames" != "" ) then
        foreach file ($filenames)
            if ( $?SPFS_DEBUG ) echo source ${startup_dir}/$file 1>&2
            source ${startup_dir}/$file
        end
    endif
endif

if ( "$#argv" != 0 ) then
    $*
    exit $?
endif

echo "* You are now in an spfs-configured shell *" 1>&2
"""
