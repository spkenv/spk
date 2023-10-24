// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

pub fn source<T>(_tmpdir: Option<&T>) -> String
where
    T: AsRef<str>,
{
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
