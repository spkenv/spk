from typing import NamedTuple, Dict, Optional, Union, Iterable, Tuple
from datetime import datetime, timezone
import unicodedata

from ._tag import TagSpec


class EnvSpec(str):
    """Env specifies a complete runtime environment that can be made up of multiple layers.
    """

    def __init__(self, spec: str) -> None:

        parse_env_spec(spec)

    @property
    def tags(self) -> Tuple[TagSpec, ...]:
        """Return the ordered set of tags that make up this environment."""
        return parse_env_spec(self)


def parse_env_spec(spec: str) -> Tuple[TagSpec, ...]:

    tags = spec.split("+")
    return tuple(TagSpec(tag) for tag in tags)
