#!/usr/bin/env sh
startup_dir="/env/etc/spenv/startup.d"
if [[ -d ${startup_dir} ]]; then
    for file in $(ls ${startup_dir}); do
        echo source ${startup_dir}/$file
        source ${startup_dir}/$file
    done
fi

if [[ "$#" -ne 0 ]]; then
    "$@"
    exit $?
fi
