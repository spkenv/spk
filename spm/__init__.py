"""The 'S' Package Manger: Convenience, clarity and speed."""

__version__ = "0.1.0"

from ._version import Version, parse_version
from ._release import Release, parse_release
from ._ident import Ident, parse_ident
from ._build_spec import BuildSpec
from ._spec import Spec, read_spec_file

from ._planner import Planner
from ._build import build, build_variants
