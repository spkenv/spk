# Copyright (c) 2022 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import os
import spdev


def inject_credentials(super_script) -> spdev.shell.Script:
    if not os.environ.get("CI"):
        return super_script()

    script = []

    # Inject github credentials
    script.append(
        spdev.shell.Command(
            "find",
            ".",
            "-name",
            "Cargo.toml",
            "|",
            "xargs",
            "sed",
            "-i",
            '"s|https://github.com|https://$GITHUB_SPFS_PULL_USERNAME:$GITHUB_SPFS_PULL_PASSWORD@github.com|"',
        )
    )

    script.extend(super_script())

    return script


class RustCrate(spdev.stdlib.components.RustCrate):
    schema = {}

    def compile_lint_script(self) -> spdev.shell.Script:
        return inject_credentials(super().compile_lint_script)

    def compile_build_script(self) -> spdev.shell.Script:
        return inject_credentials(super().compile_build_script)

    def compile_package_script(self) -> spdev.shell.Script:
        # we are not actually publishing this one so don't bother packing it
        return []


class RPMPackage(spdev.stdlib.components.RPMPackage):
    schema = {}

    def compile_build_script(self) -> spdev.shell.Script:
        # The source tarball is made in the build phase
        return inject_credentials(super().compile_build_script)
