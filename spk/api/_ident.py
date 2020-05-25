from dataclasses import dataclass, field

from ruamel import yaml

from .. import compat
from ._build import Build, parse_build


@dataclass
class Ident:
    """Ident represents a package identifier.

	The identifier is either a specific package or
	range of package versions/releases depending on the
	syntax and context
	"""

    name: str
    version: compat.Version = field(default_factory=lambda: compat.Version(""))
    build: Build = None

    def __str__(self) -> str:

        version = str(self.version)
        out = self.name
        if version:
            out += "/" + version
        if self.build:
            out += "/" + self.build.digest
        return out

    __repr__ = __str__

    def parse(self, source: str) -> None:

        name, version, build, *other = str(source).split("/") + ["", ""]

        if any(other):
            raise ValueError(f"Too many tokens in identifier: {source}")

        self.name = name
        self.version = compat.parse_version(version)
        self.build = parse_build(build) if build else None


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
