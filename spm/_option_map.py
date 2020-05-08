from typing import Dict, Any
import hashlib
import base64

from sortedcontainers import SortedDict

# given option digests are namespaced by the package itself,
# there are slim likelyhoods of collision, so we roll the dice
_DIGEST_SIZE = 7


class OptionMap(SortedDict):
    """A set of values for package build options."""

    def digest(self) -> str:

        hasher = hashlib.sha1()
        for name, value in self.items():
            hasher.update(name.encode())
            hasher.update(b"=")
            hasher.update(value.encode())
            hasher.update(bytes([0]))

        digest = hasher.digest()
        return base64.b32encode(digest)[:_DIGEST_SIZE].decode()

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "OptionMap":

        opts = OptionMap()
        for name, value in data.items():
            opts[name] = str(value)
        return opts
