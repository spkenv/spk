from typing import NamedTuple, Dict

VERSION_LATEST = "latest"


class Tag(NamedTuple):
    """Tag is a human-readable name given to a spenv layer or platform."""

    name: str
    org: str = ""
    version: str = "latest"

    @property
    def path(self) -> str:
        """Return this tag with no version number."""
        return f"{self.org}/{self.name}"

    @staticmethod
    def parse(ref_str: str) -> "Tag":
        """Parse a tag string into its parts."""
        return Tag(**_parse_dict(ref_str))


def _parse_dict(ref_str: str) -> Dict[str, str]:

    try:
        return _parse_dict_unsafe(ref_str)
    except Exception:
        raise ValueError(
            f'invalid tag "{ref_str}", must be in the form [<org>/]<name>[:<version>]'
        )


def _parse_dict_unsafe(ref_str: str) -> Dict[str, str]:
    if ":" not in ref_str:
        ref_str += ":" + VERSION_LATEST
    name, version = ref_str.rsplit(":")

    if "/" not in name:
        name = "/" + name
    org, name = name.rsplit("/", 1)

    return {"name": name, "org": org, "version": version}
