#!/bin/bash

git_repo_name=$(basename -s .git `git config --get remote.origin.url` 2>/dev/null || echo "")
current_commit_sha=$(git rev-parse HEAD 2>/dev/null || echo "")
cwd=$(pwd 2>/dev/null || echo "")
hostname=$(hostname 2>/dev/null || echo "")


printf '{"REPO": "%s", "SHA1": "%s", "CWD": "%s", "HOSTNAME": "%s"}\n' "$git_repo_name" "$current_commit_sha" "$cwd" "$hostname"
