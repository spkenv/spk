
from typing import Any, Dict, Iterator, List, MutableMapping, Optional, Set, Union
import typing

EMBEDDED: str
SRC: str
COMPATIBLE: Compatibility

def opt_from_dict(input: Dict[str, Any]) -> Option: ...
def request_from_dict(input: Dict[str, Any]) -> Request: ...

class Compatibility:
    def __init__(self, msg: str = "") -> None: ...

class Ident:
    version: Version
    build: Optional[str]

    @property
    def name(Self) -> str: ...

    def __init__(self, name: str, version: Version = None, build: str = None) -> None:...
    def is_source(self) -> bool: ...
    def set_build(self, build: str) -> None: ...
    def with_build(self, build: Optional[str]) -> Ident: ...


class Spec:
    pkg: Ident
    deprecated: bool
    build: BuildSpec
    install: InstallSpec

    @staticmethod
    def from_dict(input: Dict[str, Any]) -> Spec: ...

class BuildSpec: ...

class InstallSpec:
    requirements: List[Request]
    embedded: List[Spec]

class RangeIdent:
    version: str
    build: Optional[str]

    @property
    def name(self) ->str: ...

class PkgRequest:
    pkg: RangeIdent
    pin: Optional[str]

    @staticmethod
    def from_dict(input: Dict[str, Any]) -> PkgRequest: ...

class VarRequest:
    var: str
    pin: bool

    @property
    def value(self) -> str: ...

Request = Union[PkgRequest, VarRequest]

class PkgOpt:
    pkg: str
    default: str

    @property
    def value(self) -> Optional[str]: ...

class VarOpt:
    var: str
    default: str
    choices: Set[str]

    @property
    def value(self) -> Optional[str]: ...

Option = Union[PkgOpt, VarOpt]

class TestSpec: ...

class TagSet: ...

def parse_version(v: str) -> Version: ...

class Version:
    major: int
    minor: int
    patch: int
    tail: List[int]
    pre: TagSet
    post: TagSet

    def __init__(self, major: int = 0, minor: int = 0, patch: int = 0) -> None: ...
    @property
    def parts(self) -> List[int]: ...
    @property
    def base(self) -> str: ...
    def is_zero(self) -> bool: ...

class LocalSource:
    @staticmethod
    def from_dict(input: Dict[str, Any]) -> LocalSource: ...

class GitSource:
    @staticmethod
    def from_dict(input: Dict[str, Any]) -> GitSource: ...

class TarSource:
    @staticmethod
    def from_dict(input: Dict[str, Any]) -> TarSource: ...

class ScriptSource: ...

class OptionMap:
    @typing.overload
    def __init__(self, data: Dict[str, str]) -> None: ...
    @typing.overload
    def __init__(self, **data: str) -> None: ...
    @typing.overload
    def get(self, default: str) -> str: ...
    @typing.overload
    def get(self, default: None = None) -> Optional[str]: ...
    def copy(self) -> OptionMap: ...
    def update(self, other: OptionMap) -> None: ...

    def __getitem__(self, k: str) -> str: ...
    def __setitem__(self, k: str, v: str) -> None: ...
    def __delitem__(self, k: str) -> None: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator[str]: ...

    @property
    def digest(self) -> str: ...



class SemverRange: ...
class WildcardRange: ...
class LowestSpecifiedRange: ...
class GreaterThanRange: ...
class LessThanRange: ...
class GreaterThanOrEqualToRange: ...
class LessThanOrEqualToRange: ...
class ExactVersion: ...
class ExcludedVersion: ...
class CompatRange: ...
class VersionFilter: ...

VersionRange = Union[
    SemverRange,
    WildcardRange,
    LowestSpecifiedRange,
    GreaterThanRange,
    LessThanRange,
    GreaterThanOrEqualToRange,
    LessThanOrEqualToRange,
    ExactVersion,
    ExcludedVersion,
    CompatRange,
    VersionFilter,
]
