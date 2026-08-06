#!/bin/bash

json_escape() {
  echo -n "$1" | sed 's/\\/\\\\/g' | sed 's/"/\\"/g' | sed -z 's/\n/\\n/g' | sed -z 's/\r/\\r/g' | sed 's/\t/\\t/g'
}

declare -A metadata

# Username
metadata["user"]="${GITLAB_USER_LOGIN:-$USER}"

# Date
metadata["date"]=$(date)

# Current work directory
metadata["workdir"]=$(pwd 2>/dev/null || echo "")

# Host name
metadata["hostname"]=$(hostname 2>/dev/null || echo "")

# Git Specifics
if [ -d .git ]; then
    metadata["git.repo"]=$(git ls-remote --get-url origin || echo "")
    metadata["git.commit"]=$(git rev-parse HEAD 2>/dev/null || echo "")
    metadata["git.branch"]=$(git branch --show-current 2>/dev/null || echo "")
fi;

# CI Specifics
if [ -n "${CI_PIPELINE_ID}" ]; then
    metadata["ci_pipeline_id"]="$CI_PIPELINE_ID"
    metadata["ci_pipeline_url"]="$CI_PIPELINE_URL"
    metadata["ci_project_url"]="$CI_PROJECT_URL"
    metadata["ci_runner_id"]="$CI_RUNNER_ID"
    metadata["ci_runner_tags"]="$CI_RUNNER_TAGS"
fi;

# Build Json output
json="{"
num_elements=${#metadata[@]}
index=0
for data in "${!metadata[@]}"; do
    value=$(json_escape "${metadata[${data}]}")
    json+="\"$data\": \"$value\""
    if [[ $((index++)) -lt $((num_elements - 1)) ]]; then
        json+=", "
    fi
done
json+="}"

echo "$json"
