// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use itertools::Itertools;

use super::EnvKeyValue;

pub fn source(_environment_overrides: &[EnvKeyValue]) -> String {
    // TODO: Support environment overrides on Windows
    r#"
param (
    [string]$RunCommand
)

$spfs_startup_dir = "C:\spfs\etc\spfs\startup.d"

if (Test-Path $spfs_startup_dir) {
    $files = Get-ChildItem -Hidden -Path $spfs_startup_dir -File -Include "*.ps1" -Name | Sort-Object -Property { $_.Name }
    foreach ($file in $files) {
        . $file.Name
    }
}

if (-not ([string]::IsNullOrEmpty($RunCommand))) {
    Invoke-Expression "& $RunCommand"
    exit $LASTEXITCODE
}

if (-not ([string]::IsNullOrEmpty($env::SPFS_SHELL_MESSAGE))) {
    Write-Information "$env:SPFS_SHELL_MESSAGE"
}
"#
    .to_string()
}
