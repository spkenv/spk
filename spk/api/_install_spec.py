from typing import Dict, List, Any, Iterable
from dataclasses import dataclass, field

from ._request import Request
from ._ident import Ident


@dataclass
class InstallSpec:
    """A set of structured installation parameters for a package."""

    requirements: List[Request] = field(default_factory=list)

    def upsert_requirement(self, request: Request) -> None:
        """Add or update a requirement to the set of installation requirements.

        If a request exists for the same package, it is replaced with the given
        one. Otherwise the new request is appended to the list.
        """
        for i, other in enumerate(self.requirements):
            if other.pkg.name == request.pkg.name:
                self.requirements[i] = request
                return
        else:
            self.requirements.append(request)

    def to_dict(self) -> Dict[str, Any]:
        return {"requirements": list(r.to_dict() for r in self.requirements)}

    def render_all_pins(self, resolved: Iterable[Ident]) -> None:
        """Render all requests with a package pin using the given resolved packages."""

        by_name = dict((pkg.name, pkg) for pkg in resolved)
        for i, request in enumerate(self.requirements):
            if not request.pin:
                continue
            if request.pkg.name not in by_name:
                raise ValueError(f"Pinned package not present: {request.pkg.name}")
            self.requirements[i] = request.render_pin(by_name[request.pkg.name])

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "InstallSpec":

        spec = InstallSpec()

        requirements = data.pop("requirements", [])
        if requirements:
            spec.requirements = list(Request.from_dict(r) for r in requirements)

        if len(data):
            raise ValueError(
                f"unrecognized fields in spec.install: {', '.join(data.keys())}"
            )

        return spec
