import abc
from typing import List

from .. import api
from . import graph


def default_validators() -> List["Validator"]:
    return [
        DeprecationValidator(),
        PkgRequestsValidator(),
        VarRequirementsValidator(),
        PkgRequirementsValidator(),
        OptionsValidator(),
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
            return api.Compatibility("Package version is deprecated")
        if request.pkg.build == spec.pkg.build:
            return api.COMPATIBLE
        return api.Compatibility(
            "Build is deprecated and was not specifically requested"
        )


class BinaryOnly(Validator):
    """Enforces the resolution of binary packages only, denying new builds from source."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:
        if spec.pkg.build is None:
            return api.Compatibility("Only binary packages are allowed")
        request = state.get_merged_request(spec.pkg.name)
        if spec.pkg.build.is_source() and request.pkg.build != spec.pkg.build:
            return api.Compatibility("Only binary packages are allowed")
        return api.COMPATIBLE


class PkgRequestsValidator(Validator):
    """Ensures that a package meets all requested version criteria."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:

        try:
            request = state.get_merged_request(spec.pkg.name)
        except KeyError:
            return api.Compatibility("package was not requested [INTERNAL ERROR]")
        return request.is_satisfied_by(spec)


class OptionsValidator(Validator):
    """Ensures that a package is compatible with all defined and requested options."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:
        compat = spec.build.validate_options(spec.pkg.name, state.get_option_map())
        if not compat:
            return compat

        return api.COMPATIBLE


class PkgRequirementsValidator(Validator):
    """Validates that the pkg install requirements do not conflict with the existing resolve."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:

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
                return api.Compatibility(f"Conflicting install requirement: {err}")

            try:
                resolved = state.get_current_resolve(request.pkg.name)
            except KeyError:
                continue
            compat = request.is_satisfied_by(resolved)
            if not compat:
                return api.Compatibility(
                    f"Conflicting install requirement: '{request.pkg.name}' {compat}"
                )

        return api.COMPATIBLE


class VarRequirementsValidator(Validator):
    """Validates that the var install requirements do not conflict with the existing options."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:

        options = state.get_option_map()
        for request in spec.install.requirements:
            if not isinstance(request, api.VarRequest):
                continue

            if request.var not in options:
                continue

            current = options[request.var]
            if request.value != current:
                return api.Compatibility(
                    f"Conflicting var install requirement '{request.var}': wanted '{request.value}', found '{current}'"
                )
        return api.COMPATIBLE
