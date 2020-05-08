from typing import Dict, List, Any
from dataclasses import dataclass, field

from ._option_map import OptionMap


@dataclass
class BuildSpec:
    """A set of structured inputs to build a package."""

    script: str = "sh ./build.sh"
    variants: List[OptionMap] = field(default_factory=list)

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "BuildSpec":

        bs = BuildSpec()
        if "script" in data:
            script = data.pop("script")
            if isinstance(script, list):
                script = "\n".join(script)
            bs.script = script

        for variant in data.pop("variants", []):
            bs.variants.append(OptionMap.from_dict(variant))

        if len(data):
            raise ValueError(
                f"unrecognized fields in spec.build: {', '.join(data.keys())}"
            )

        return bs
