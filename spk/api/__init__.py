from ._option_map import OptionMap, host_options
from ._version import Version, parse_version, VERSION_SEP
from ._compat import Compat, parse_compat
from ._build import Build, parse_build, SRC
from ._install_spec import InstallSpec
from ._ident import Ident, parse_ident
from ._version_range import (
    VersionRange,
    VersionFilter,
    VERSION_RANGE_SEP,
    parse_version_range,
)
from ._build_spec import BuildSpec, VarSpec, opt_from_dict
from ._source_spec import SourceSpec
from ._spec import Spec, read_spec_file, read_spec, write_spec
from ._request import Request, parse_ident_range, PreReleasePolicy
