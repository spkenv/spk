# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import List
import sys

import structlog
import colorama

import spkrs
from spkrs.exec import resolve_runtime_layers
from . import solve, storage, io, build, api

_LOGGER = structlog.get_logger("spk.exec")


def build_required_packages(solution: solve.Solution) -> solve.Solution:
    """Build any packages in the given solution that need building.

    Returns:
      solve.Solution: a new solution of only binary packages
    """

    local_repo = storage.local_repository()
    repos = solution.repositories()
    options = solution.options()
    compiled_solution = solve.Solution(options)
    for item in solution.items():
        if not item.is_source_build():
            compiled_solution.add(*item)
            continue

        req, spec, source = item
        _LOGGER.info(
            f"Building: {io.format_ident(spec.pkg)} for {io.format_options(options)}"
        )
        spec = (
            build.BinaryPackageBuilder.from_spec(source)  # type: ignore
            .with_repositories(repos)
            .with_options(options)
            .build()
        )
        source = (local_repo, local_repo.get_package(spec.pkg))
        compiled_solution.add(req, spec, source)
    return compiled_solution


def setup_current_runtime(solution: solve.Solution) -> None:
    """Modify the active spfs runtime to include exactly the packges in the given solution."""

    _runtime = spkrs.active_runtime()
    stack = resolve_runtime_layers(solution)
    spkrs.reconfigure_runtime(stack=stack)
