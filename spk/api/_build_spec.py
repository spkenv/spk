from typing import Dict, List, Any
from dataclasses import dataclass, field

from ._option_map import OptionMap


@dataclass
class BuildSpec:
    """A set of structured inputs to build a package."""

    script: str = "sh ./build.sh"
    variants: List[OptionMap] = field(default_factory=lambda: [OptionMap()])

    def to_dict(self) -> Dict[str, Any]:
        return {
            "script": self.script.splitlines(),
            "variants": list(dict(v) for v in self.variants),
        }

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "BuildSpec":

        bs = BuildSpec()
        if "script" in data:
            script = data.pop("script")
            if isinstance(script, list):
                script = "\n".join(script)
            bs.script = script

        variants = data.pop("variants", [])
        if variants:
            bs.variants = list(OptionMap.from_dict(v) for v in variants)

        if len(data):
            raise ValueError(
                f"unrecognized fields in spec.build: {', '.join(data.keys())}"
            )

        return bs
