from typing import NamedTuple, Dict


class Tag(NamedTuple):

    name: str
    org: str = ""
    version: str = "latest"

    @property
    def path(self) -> str:
        return f"{self.org}/{self.name}"

    @staticmethod
    def parse(ref_str: str) -> "Tag":

        return Tag(**_parse_dict(ref_str))


def _parse_dict(ref_str: str) -> Dict[str, str]:

    if ":" not in ref_str:
        ref_str += ":latest"
    name, version = ref_str.rsplit(":", 1)

    if "/" not in name:
        name = "/" + name
    org, name = name.rsplit("/", 1)

    return {"name": name, "org": org, "version": version}
