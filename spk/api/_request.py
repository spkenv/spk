from typing import Dict, List, Any, Union
from dataclasses import dataclass, field

from ._version import Version, parse_version
from ._ident import Ident, parse_ident


@dataclass
class Request:
    """A desired package and set of restrictions."""

    pkg: Ident

    def clone(self) -> "Request":

        return Request.from_dict(self.to_dict())

    def is_version_applicable(self, version: Union[str, Version]) -> bool:
        """Return true if the given version number is applicable to this request.

        This is used a cheap preliminary way to prune package
        versions that are not going to satisfy the request without
        needing to load the whole package spec.
        """

        if not isinstance(version, Version):
            version = parse_version(version)

        return self.pkg.version.is_satisfied_by(version)

    def restrict(self, other: "Request") -> None:

        self.pkg.restrict(other.pkg)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "pkg": self.pkg,
        }

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "Request":

        try:
            req = Request(parse_ident(data.pop("pkg")))
        except KeyError as e:
            raise ValueError(f"Missing required key in package request: {e}")

        if len(data):
            raise ValueError(
                f"unrecognized fields in package request: {', '.join(data.keys())}"
            )

        return req
