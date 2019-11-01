from typing import NamedTuple, Dict, Optional, Union, Iterable
from datetime import datetime, timezone
import hashlib
import socket
import getpass


_tag_str_template = """\
   tag: {org}/{name}
digest: {digest}
target: {target}
parent: {parent}
  user: {user}
  time: {time}

{message}
"""


class Tag(NamedTuple):
    """Tag links a human name to a storage object at some point in time.

    Much like a commit, tags form a linked-list of entries to track history.
    Time should always be in UTC.
    """

    org: str
    name: str
    target: str
    parent: Optional[str] = None
    user: str = f"{getpass.getuser()}@{socket.gethostname()}"
    time: datetime = datetime.now().replace(microsecond=0).astimezone()

    def __str__(self) -> str:

        dict_values = self._asdict()
        dict_values["digest"] = self.digest
        dict_values["time"] = dict_values["time"].strftime("%A, %B %d, %Y - %I:%M%p")
        return _tag_str_template.format(**dict_values)

    @property
    def path(self) -> str:
        """Return this tag with no version number."""
        return f"{self.org}/{self.name}"

    @property
    def digest(self) -> str:

        hasher = hashlib.sha256(self.encode())
        return hasher.hexdigest()

    def encode(self) -> bytes:

        encoded = b""
        encoded += self.org.encode("utf-8") + b"\t"
        encoded += self.name.encode("utf-8") + b"\t"
        encoded += self.target.encode("utf-8") + b"\t"
        encoded += self.user.encode("utf-8") + b"\t"
        time = self.time
        if not time.tzinfo:
            time = time.astimezone()
        encoded += time.isoformat().encode("utf-8") + b"\t"
        if self.parent is not None:
            encoded += self.parent.encode("utf-8")
        return encoded


def decode_tag(encoded: bytes) -> Tag:
    """Decode a previously encoded tag value."""

    fields = encoded.split(b"\t")
    org = fields.pop(0).decode("utf-8")
    name = fields.pop(0).decode("utf-8")
    target = fields.pop(0).decode("utf-8")
    user = fields.pop(0).decode("utf-8")
    time = datetime.fromisoformat(fields.pop(0).decode("utf-8")).astimezone()
    if fields:
        parent = fields.pop(0).decode("utf-8")
    return Tag(org=org, name=name, target=target, user=user, time=time, parent=parent)


class TagSpec(NamedTuple):
    """TagSpec identifies a tag within a tag stream.

    The tag spec represents a string specifier or the form:
        tag[version]
    where tag is a tag string, and version is an integer version number
    specifying a position in the tag stream. The integer '0' always refers to
    the latest tag in the stream. All other version numbers must be negative,
    referring to the number of steps back in the version stream to go.
        eg: spi/main[0]   # latest tag in the spi/main stream
            spi/main[-4]  # the tag 4 versions behind the latest in the stream
    """

    name: str
    org: str = ""
    version: int = 0

    @property
    def path(self) -> str:
        """Return this tag with no version number."""
        if self.org:
            return f"{self.org}/{self.name}"
        return self.name


def parse_tag_spec(spec: str) -> TagSpec:
    """Parse a tag string into its parts."""
    return TagSpec(**_parse_spec_dict(spec))  # type: ignore


def _parse_spec_dict(spec: str) -> Dict[str, Union[str, int]]:

    try:
        return _parse_spec_dict_unsafe(spec)
    except Exception:
        raise ValueError(
            f'invalid tag "{spec}", must be in the form <org>/<name>[<version>]'
        )


def _parse_spec_dict_unsafe(spec: str) -> Dict[str, Union[str, int]]:
    if "[" not in spec:
        spec += "[0]"
    name, version = spec.split("[", 1)
    version = version.rstrip("]")

    if "/" not in name:
        name = "/" + name
    org, name = name.rsplit("/", 1)

    return {"name": name, "org": org, "version": int(version)}
