# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import spkrs

from . import api, storage
from ._publish import Publisher


def test_publish_no_version_spec(tmprepo: storage.Repository) -> None:

    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0/BGSHW3CN"})
    tmprepo.publish_package(spec, spkrs.EMPTY_DIGEST)

    publisher = (
        Publisher().with_source(tmprepo).with_target(spkrs.storage.mem_repository())
    )
    publisher.publish("my-pkg/1.0.0")
