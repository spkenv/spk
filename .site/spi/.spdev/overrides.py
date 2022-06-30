# Copyright (c) 2022 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import os
import spdev
from typing import Sequence


class RawCommand(spdev.shell.Command):
    def bash_source(self) -> str:
        cmd = [self._exe]
        cmd.extend(self._args)

        # Don't quote my args for me, thanks.

        return " ".join(cmd)


def inject_credentials(super_script_list: spdev.shell.Script) -> spdev.shell.Script:
    if not os.environ.get("CI"):
        return super_script_list

    script = []

    # Inject github credentials
    script.append(
        RawCommand(
            "git",
            "ls-files",
            # Handle running this command from a subdirectory of the project.
            '"$(git rev-parse --show-toplevel)/**Cargo.toml"',
            "|",
            "xargs",
            "-n",
            "1",
            "sed",
            "-i",
            '"s|https://github.com|https://$GITHUB_SPFS_PULL_USERNAME:$GITHUB_SPFS_PULL_PASSWORD@github.com|"',
        )
    )

    script.extend(super_script_list)

    return script


class RustCrate(spdev.stdlib.components.RustCrate):
    schema = {}

    def compile_lint_script(self) -> spdev.shell.Script:
        return inject_credentials(super().compile_lint_script())

    def compile_build_script(self) -> spdev.shell.Script:
        return inject_credentials(super().compile_build_script())

    def compile_package_script(self) -> spdev.shell.Script:
        # we are not actually publishing this one so don't bother packing it
        return []

    def compile_test_script(self) -> spdev.shell.Script:
        if self.name != "spk":
            return inject_credentials(super().compile_test_script())

        return inject_credentials(
            [
                spdev.shell.Chdir(self.path()),
                spdev.shell.Command("make", "test"),
            ]
        )


class RPMPackage(spdev.stdlib.components.RPMPackage):
    schema = {}

    def compile_build_script(self) -> spdev.shell.Script:
        # The source tarball is made in the build phase
        return inject_credentials(super().compile_build_script())
