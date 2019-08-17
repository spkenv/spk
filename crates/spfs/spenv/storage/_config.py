import os
from typing import NamedTuple


class Config(NamedTuple):

    repo_root: str = os.path.expanduser("~/.local/share/spenv")
