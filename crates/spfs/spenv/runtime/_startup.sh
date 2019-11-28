#!/usr/bin/env sh
if [[ -f ~/.bashrc ]]; then
    source ~/.bashrc
fi
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

echo "* You are now in an spenv-configured shell *"
