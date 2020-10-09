from multiprocessing import Value
from spfs import encoding
from typing import NamedTuple, Dict, Optional, Union, Iterable, Tuple
from datetime import datetime, timezone
import unicodedata

from ._tag import TagSpec


class EnvSpec(str):
    """Env specifies a complete runtime environment that can be made up of multiple layers.
    """

    def __new__(cls, spec: str) -> None:

        tuple(parse_env_spec(spec))
        return str.__new__(cls, spec)  # type: ignore

    @property
    def items(self) -> Tuple[Union[TagSpec, encoding.Digest], ...]:
        """Return the ordered set of tags that make up this environment."""
        return tuple(parse_env_spec(self))


def parse_env_spec(spec: str) -> Iterable[Union[TagSpec, encoding.Digest]]:
    """Return the items identified in an environment spec string.

    >>> list(parse_env_spec("sometag~1+my-other-tag"))
    ['sometag~1', 'my-other-tag']
    >>> list(parse_env_spec("3YDG35SUMJS67N2QPQ4NQCYJ6QGKMEB5H4MHC76VRGMRWBRBLFHA====+my-tag"))
    [3YDG35SUMJS67N2QPQ4NQCYJ6QGKMEB5H4MHC76VRGMRWBRBLFHA====, 'my-tag']
    """

    layers = tuple(filter(None, spec.split("+")))
    if not layers:
        raise ValueError("Must specify at least one digest or tag")
    for layer in layers:
        try:
            yield encoding.parse_digest(layer)
        except ValueError:
            yield TagSpec(layer)
