from dataclasses import dataclass, field

from ruamel import yaml

from .. import compat
from ._release import Release, parse_release


@dataclass
class Ident:
    """Ident represents a package identifier.

	The identifier is either a specific package or
	range of package versions/releases depending on the
	syntax and context
	"""

    name: str
    version: compat.Version = field(default_factory=lambda: compat.Version(""))
    release: Release = field(default_factory=lambda: Release(""))

    def __str__(self) -> str:

        version = str(self.version)
        release = str(self.release)
        out = self.name
        if version:
            out += "/" + version
        if release:
            out += "/" + release
        return out

    __repr__ = __str__

    def parse(self, source: str) -> None:

        name, version, release, *other = str(source).split("/") + ["", ""]

        if any(other):
            raise ValueError(f"Too many tokens in identifier: {source}")

        self.name = name
        self.version = compat.parse_version(version)
        self.release = parse_release(release)


def parse_ident(source: str) -> Ident:
    """Parse a package identifier string."""
    ident = Ident("")
    ident.parse(source)
    return ident


yaml.Dumper.add_representer(
    Ident,
    lambda dumper, data: yaml.representer.SafeRepresenter.represent_str(
        dumper, str(data)
    ),
)

yaml.SafeDumper.add_representer(
    Ident,
    lambda dumper, data: yaml.representer.SafeRepresenter.represent_str(
        dumper, str(data)
    ),
)
