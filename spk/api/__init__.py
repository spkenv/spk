# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from ._name import InvalidNameError
from ._option_map import OptionMap, host_options
from ._version import Version, parse_version, VERSION_SEP, InvalidVersionError
from ._compat import Compat, parse_compat, Compatibility, COMPATIBLE, CompatRule
from ._build import Build, parse_build, SRC, EMBEDDED, InvalidBuildError
from ._ident import Ident, parse_ident, validate_name
from ._version_range import (
    VersionRange,
    VersionFilter,
    VERSION_RANGE_SEP,
    parse_version_range,
)
from ._build_spec import BuildSpec, opt_from_dict, VarOpt, PkgOpt, Option, Inheritance
from ._source_spec import SourceSpec, LocalSource, GitSource, TarSource, ScriptSource
from ._test_spec import TestSpec
from ._request import (
    Request,
    PkgRequest,
    VarRequest,
    parse_ident_range,
    PreReleasePolicy,
    InclusionPolicy,
    RangeIdent,
)
from ._spec import (
    InstallSpec,
    Spec,
    read_spec_file,
    read_spec,
    write_spec,
    save_spec_file,
)
