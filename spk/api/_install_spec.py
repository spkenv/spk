from typing import Dict, List, Any, Iterable

from ._request import Request
from ._ident import Ident
from ._env_spec import Env


class InstallSpec(Env):
    """A set of structured installation parameters for a package."""

    def to_dict(self) -> Dict[str, Any]:
        return {
            "requirements": list(r.to_dict() for r in self.requirements),
        }

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

        data["env"] = "install"
        env = Env.from_dict(data)
        return InstallSpec(env.name, env.requirements)
