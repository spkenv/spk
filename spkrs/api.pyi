
from typing import Dict, Optional, Union
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

class PkgOpt: ...

class VarOpt: ...

Option = Union[PkgOpt, VarOpt]

class TestSpec: ...

class Version: ...

class LocalSource: ...

class GitSource: ...

class TarSource: ...

class ScriptSource: ...

class OptionMap:
    @typing.overload
    def __init__(self, data: Dict[str, str]) -> None: ...
    @typing.overload
    def __init__(self, **data: str) -> None: ...
