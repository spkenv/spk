from typing import Dict, Any
import hashlib
import base64
import platform
import os

import distro
from ruamel import yaml
from sortedcontainers import SortedDict

# given option digests are namespaced by the package itself,
# there are slim likelyhoods of collision, so we roll the dice
# also must be a multiple of 8 to be decodable wich is generally
# a nice way to handle validation / and 16 is a lot
_DIGEST_SIZE = 8


class OptionMap(SortedDict):
    """A set of values for package build options."""

    def __str__(self) -> str:

        return f"{{{', '.join(f'{n}: {v}' for n, v in self.items())}}}"

    __repr__ = __str__

    def copy(self) -> "OptionMap":
        return OptionMap(**self)

    def digest(self) -> str:

        hasher = hashlib.sha1()
        for name, value in self.items():
            hasher.update(name.encode())
            hasher.update(b"=")
            hasher.update(value.encode())
            hasher.update(bytes([0]))

        digest = hasher.digest()
        return base64.b32encode(digest)[:_DIGEST_SIZE].decode()

    def global_options(self) -> "OptionMap":

        return OptionMap(o for o in self.items() if "." not in o[0])

    def package_options_without_global(self, name: str) -> "OptionMap":
        """Return the set of options given for the specific named package."""

        prefix = name + "."
        options = OptionMap()
        for key, value in self.items():
            if key.startswith(prefix):
                options[key[len(prefix) :]] = value
        return options

    def package_options(self, name: str) -> "OptionMap":
        """Return the set of options relevant to the named package."""

        options = self.global_options()
        options.update(self.package_options_without_global(name))
        return options

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "OptionMap":

        opts = OptionMap()
        for name, value in data.items():
            opts[name] = str(value)
        return opts

    def to_environment(self, base: Dict[str, str] = None) -> Dict[str, str]:
        """Return the data of these options as environment variables.

        If base is not given, use current os environment.
        """

        if base is None:
            base = dict(os.environ)
        else:
            base = base.copy()

        for name, value in self.items():
            var_name = f"SPK_OPT_{name}"
            base[var_name] = value
        return base


def host_options() -> OptionMap:
    """Detect and return the default options for the current host system"""

    opts = OptionMap(arch=platform.machine(), os=platform.system().lower())

    info = distro.info()
    distro_name = info["id"]
    opts["distro"] = distro_name
    opts[distro_name] = info["version"]

    return opts
