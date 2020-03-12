from typing import Tuple, BinaryIO, Any
from datetime import datetime
import socket
import getpass
import unicodedata

import pytz.reference

from .. import encoding


_tag_str_template = """\
   tag: {tag}
digest: {digest}
target: {target}
parent: {parent}
  user: {user}
  time: {time}
"""


class Tag(encoding.Encodable):
    """Tag links a human name to a storage object at some point in time.

    Much like a commit, tags form a linked-list of entries to track history.
    Time should always be in UTC.
    """

    __fields__ = [
        "org",
        "name",
        "target",
        "parent",
        "user",
        "time",
    ]

    def __init__(
        self,
        org: str,
        name: str,
        target: encoding.Digest,
        parent: encoding.Digest = encoding.NULL_DIGEST,
        user: str = f"{getpass.getuser()}@{socket.gethostname()}",
        time: datetime = None,
    ):

        # we want to ensure these components
        # can build a valid tag spec
        spec = build_tag_spec(name=name, org=org)
        self.org = spec.org
        self.name = spec.name
        self.target = target
        self.parent = parent
        self.user = user
        if time is None:
            self.time = datetime.now().replace(microsecond=0).astimezone()
        else:
            self.time = time

    def __str__(self) -> str:

        dict_values = {
            "tag": f"{self.org}/{self.name}",
            "digest": self.digest().str(),
            "target": self.target.str(),
            "parent": self.parent.str(),
            "user": self.user,
            "time": self.time.strftime("%A, %B %d, %Y - %I:%M%p"),
        }

        return _tag_str_template.format(**dict_values)

    __repr__ = __str__

    @property
    def path(self) -> str:
        """Return this tag with no version number."""
        if self.org:
            return f"{self.org}/{self.name}"
        return self.name

    def encode(self, writer: BinaryIO) -> None:

        encoding.write_string(writer, self.org)
        encoding.write_string(writer, self.name)
        encoding.write_digest(writer, self.target)
        encoding.write_string(writer, self.user)
        time = self.time
        if not time.tzinfo:
            time = time.astimezone(pytz.reference.LocalTimezone())
        encoding.write_string(writer, time.isoformat())
        encoding.write_digest(writer, self.parent)

    @classmethod
    def decode(cls, reader: BinaryIO) -> "Tag":

        return Tag(
            org=encoding.read_string(reader),
            name=encoding.read_string(reader),
            target=encoding.read_digest(reader),
            user=encoding.read_string(reader),
            time=datetime.fromisoformat(encoding.read_string(reader)),
            parent=encoding.read_digest(reader),
        )


class TagSpec(str):
    """TagSpec identifies a tag within a tag stream.

    The tag spec represents a string specifier or the form:
        [org /] name [~ version]
    where org is a slash-separated path denoting a group-like organization for the tag
    where name is the tag identifier, can only include alphanumeric, '-', ':', '.', and '_'
    where version is an integer version number specifying a position in the tag
    stream. The integer '0' always refers to the latest tag in the stream. All other
    version numbers must be negative, referring to the number of steps back in
    the version stream to go.
        eg: spi/main   # latest tag in the spi/main stream
            spi/main~0 # latest tag in the spi/main stream
            spi/main~4 # the tag 4 versions behind the latest in the stream
    """

    def __init__(self, spec: str) -> None:

        split_tag_spec(spec)

    @property
    def org(self) -> str:
        return split_tag_spec(self)[0]

    @property
    def name(self) -> str:
        return split_tag_spec(self)[1]

    @property
    def version(self) -> int:
        return split_tag_spec(self)[2]

    @property
    def path(self) -> str:
        """Return this tag with no version number."""
        org = self.org
        if org:
            return f"{org}/{self.name}"
        return self.name


def build_tag_spec(name: str, org: str = "", version: int = 0) -> TagSpec:

    path = name
    if org:
        path = org + "/" + name
    spec = path
    if version != 0:
        spec = path + "~" + str(version)
    return TagSpec(spec)


def split_tag_spec(spec: str) -> Tuple[str, str, int]:

    parts = spec.rsplit("/", 1)
    if len(parts) == 1:
        parts = [""] + parts

    org, name_version = parts

    parts = name_version.split("~", 1)
    if len(parts) == 1:
        parts += ["0"]

    name, version = parts

    if not name:
        raise ValueError("tag name cannot be empty: " + spec)

    index = _find_org_error(org)
    if index >= 0:
        err_str = f"{org[:index]} > {org[index]} < {org[index+1:]}"
        raise ValueError(f"invalid tag org at pos {index}: {err_str}")
    index = _find_name_error(name)
    if index >= 0:
        err_str = f"{name[:index]} > {name[index]} < {name[index+1:]}"
        raise ValueError(f"invalid tag name at pos {index}: {err_str}")
    index = _find_version_error(version)
    if index >= 0:
        err_str = f"{version[:index]} > {version[index]} < {version[index+1:]}"
        raise ValueError(f"invalid tag version at pos {index}: {err_str}")

    return org, name, int(version)


_NAME_UTF_CATEGORIES = (
    "Ll",  # letter lower
    "Lu",  # letter upper
    "Pd",  # punctuation dash
    "Nd",  # number digit
)
_NAME_UTF_NAMES = (unicodedata.name("_"), unicodedata.name("."))


def _find_name_error(org: str) -> int:

    return _validate_source_str(org, _NAME_UTF_CATEGORIES, _NAME_UTF_NAMES)


_ORG_UTF_CATEGORIES = _NAME_UTF_CATEGORIES
_ORG_UTF_NAMES = _NAME_UTF_NAMES + (unicodedata.name("/"),)


def _find_org_error(org: str) -> int:

    return _validate_source_str(org, _ORG_UTF_CATEGORIES, _ORG_UTF_NAMES)


_VERSION_UTF_CATEGORIES = ("Nd",)  # digits only
_VERSION_UTF_NAMES: Tuple[str, ...] = tuple()


def _find_version_error(version: str) -> int:

    return _validate_source_str(version, _VERSION_UTF_CATEGORIES, _VERSION_UTF_NAMES)


def _validate_source_str(
    source: str, valid_categories: Tuple[str, ...], valid_names: Tuple[str, ...]
) -> int:

    i = -1
    while i < len(source) - 1:
        i += 1
        char = source[i]
        category = unicodedata.category(char)
        if category in valid_categories:
            continue
        name = unicodedata.name(char)
        if name in valid_names:
            continue
        return i
    return -1
