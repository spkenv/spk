#!/usr/bin/env sh
startup_dir="/env/etc/spenv/startup.d"
if [[ -d ${startup_dir} ]]; then
    for file in ${startup_dir}/*.sh; do
        echo source $file
        source $file
    done
fi

if [[ "$#" -ne 0 ]]; then
    "$@"
    exit $?
fi
