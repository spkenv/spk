#!/usr/bin/env csh
if ( -f ~/.tcshrc ) then
    source ~/.tcshrc
else if ( -f ~/.cshrc ) then
    source ~/.cshrc
endif

set startup_dir="/env/etc/spenv/startup.d"
if ( -d "${startup_dir}" != 0 ) then
    set filenames=`ls $startup_dir | grep '\.csh$'`
    if ( "$filenames" != "" ) then
        foreach file ($filenames)
            echo source ${startup_dir}/$file
            source ${startup_dir}/$file
        end
    endif
endif

if ( "$#argv" != 0 ) then
    $*
    exit $?
endif

echo "* You are now in an spenv-configured shell *"
