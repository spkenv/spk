from typing import Dict, List, Any
from dataclasses import dataclass, field

from ._request import Request


@dataclass
class InstallSpec:
    """A set of structured installation parameters for a package."""

    requirements: List[Request] = field(default_factory=list)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "requirements": list(r.to_dict() for r in self.requirements),
        }

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
