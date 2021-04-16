# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import io

import spk

from ._cmd_new import TEMPLATE


def test_template_is_valid() -> None:

    spec = TEMPLATE.format(name="my-package")
    spk.api.read_spec(io.StringIO(spec))
