# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import io

import py.path

import spk
from ._cmd_new import TEMPLATE


def test_template_is_valid(tmpdir: py.path.local) -> None:

    spec = TEMPLATE.format(name="my-package")
    spec_file = tmpdir.join("file")
    spec_file.write(spec)
    spk.api.read_spec_file(spec_file.strpath)
