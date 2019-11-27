#!/usr/bin/env sh
startup_dir="/env/etc/spenv/startup.d"
if [[ -d ${startup_dir} ]]; then
    filenames=$(ls $startup_dir | grep '\.sh$')
    if [[ ! -z "$filenames" ]]; then
        for file in $filenames; do
            echo source $startup_dir/$file
            source $startup_dir/$file
        done
    fi
fi

if [[ "$#" -ne 0 ]]; then
    "$@"
    exit $?
fi
