import io

import spk

from ._cmd_new import TEMPLATE


def test_template_is_valid() -> None:

    spec = TEMPLATE.format(name="my-package")
    spk.api.read_spec(io.StringIO(spec))
