
from typing import Dict, List, Optional, Set, Union
import typing


class Ident:
    version: Version

    @property
    def name(Self) -> str: ...


class Spec:
    pkg: Ident
    deprecated: bool
    build: BuildSpec
    install: InstallSpec

class BuildSpec: ...

class InstallSpec: ...

class RangeIdent: ...

class PkgRequest:
    pkg: RangeIdent
    pin: Optional[str]

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

class Version:
    major: int
    minor: int
    patch: int
    tail: List[int]
    pre: TagSet
    post: TagSet

class LocalSource: ...

class GitSource: ...

class TarSource: ...

class ScriptSource: ...

class OptionMap:
    @typing.overload
    def __init__(self, data: Dict[str, str]) -> None: ...
    @typing.overload
    def __init__(self, **data: str) -> None: ...
