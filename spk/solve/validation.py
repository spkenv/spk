# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import abc
from typing import List

from .. import api
from . import graph


def default_validators() -> List["Validator"]:
    return [
        DeprecationValidator(),
        PkgRequestsValidator(),
        OptionsValidator(),
        VarRequirementsValidator(),
        PkgRequirementsValidator(),
        EmbeddedPackageValidator(),
    ]


class Validator(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:
        """Check if the given package is appropriate for the provided state."""
        ...


class DeprecationValidator(Validator):
    """Ensures that deprecated packages are not included unless specifically requested."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:
        if not spec.deprecated:
            return api.COMPATIBLE
        request = state.get_merged_request(spec.pkg.name)
        if spec.pkg.build is None and spec.deprecated:
            return api.Compatibility("package version is deprecated")
        if request.pkg.build == spec.pkg.build:
            return api.COMPATIBLE
        return api.Compatibility("build is deprecated (and not requested exactly)")


class BinaryOnly(Validator):
    """Enforces the resolution of binary packages only, denying new builds from source."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:
        if spec.pkg.build is None:
            return api.Compatibility("only binary packages are allowed")
        request = state.get_merged_request(spec.pkg.name)
        if spec.pkg.build.is_source() and request.pkg.build != spec.pkg.build:
            return api.Compatibility("only binary packages are allowed")
        return api.COMPATIBLE


class PkgRequestsValidator(Validator):
    """Ensures that a package meets all requested version criteria."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:

        try:
            request = state.get_merged_request(spec.pkg.name)
        except KeyError:
            return api.Compatibility("package was not requested [INTERNAL ERROR]")
        # the initial check is more general and provides more user
        # friendly error messages that we'd like to get
        compat = request.is_version_applicable(spec.pkg.version)
        if compat:
            compat = request.is_satisfied_by(spec)
        return compat


class OptionsValidator(Validator):
    """Ensures that a package is compatible with all requested options."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:

        options = state.get_option_map()
        options = spec.resolve_all_options(options)
        for request in state.var_requests:
            compat = spec.satisfies_var_request(request)
            if not compat:
                return api.Compatibility(f"doesn't satisfy requested option: {compat}")

        return api.COMPATIBLE


class PkgRequirementsValidator(Validator):
    """Validates that the pkg install requirements do not conflict with the existing resolve."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:

        if spec.pkg.is_source():
            # source packages are not being "installed" so requests don't matter
            return api.COMPATIBLE

        for request in spec.install.requirements:
            if not isinstance(request, api.PkgRequest):
                continue
            try:
                existing = state.get_merged_request(request.pkg.name)
                existing.restrict(request)
                request = existing
            except KeyError:
                continue
            except ValueError as err:
                return api.Compatibility(f"conflicting requirement: {err}")

            try:
                resolved = state.get_current_resolve(request.pkg.name)
            except KeyError:
                continue
            compat = resolved.satisfies_pkg_request(request)
            if not compat:
                return api.Compatibility(
                    f"conflicting requirement: '{request.pkg.name}' {compat}"
                )

        return api.COMPATIBLE


class VarRequirementsValidator(Validator):
    """Validates that the var install requirements do not conflict with the existing options."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:

        if spec.pkg.is_source():
            # source packages are not being "installed" so requests don't matter
            return api.COMPATIBLE

        options = state.get_option_map()
        for request in spec.install.requirements:
            if not isinstance(request, api.VarRequest):
                continue

            for name, value in options.items():
                if name != request.var and not name.endswith("." + request.var):
                    continue
                if value == "":
                    # empty option values do not provide a valuable opinion on the resolve
                    continue
                if request.value != value:
                    return api.Compatibility(
                        f"package wants {request.var}={request.value}, resolve has {name}={value}"
                    )
        return api.COMPATIBLE


class EmbeddedPackageValidator(Validator):
    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:

        if spec.pkg.is_source():
            # source packages are not being "installed" so embedded pkgs are not relevant
            return api.COMPATIBLE

        for embedded in spec.install.embedded:
            try:
                existing = state.get_merged_request(embedded.pkg.name)
            except KeyError:
                continue

            compat = existing.is_satisfied_by(embedded)
            if not compat:
                return api.Compatibility(
                    f"embedded package '{embedded.pkg}' is incompatible: {compat}"
                )

        return api.COMPATIBLE
