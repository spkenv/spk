from typing import NamedTuple, Optional
import os
import errno
import configparser

from . import storage

_DEFAULTS = {"storage": {"root": os.path.expanduser("~/.local/share/spenv")}}
_CONFIG: Optional["Config"] = None


class Config(configparser.ConfigParser):
    def __init__(self) -> None:
        super(Config, self).__init__()

    @property
    def storage_root(self) -> str:
        return str(self["storage"]["root"])

    def get_repository(self) -> storage.Repository:

        return storage.ensure_repository(self.storage_root)


def get_config() -> Config:

    global _CONFIG
    if _CONFIG is None:
        _CONFIG = load_config()
    return _CONFIG


def load_config() -> Config:

    user_config = os.path.expanduser("~/.config/spenv/spenv.conf")
    system_config = "/etc/spenv.conf"

    config = Config()
    config.read_dict(_DEFAULTS)
    try:
        with open(system_config, "r", encoding="utf-8") as f:
            config.read_file(f, source=system_config)
    except OSError as e:
        if e.errno != errno.ENOENT:
            raise
    try:
        with open(user_config, "r", encoding="utf-8") as f:
            config.read_file(f, source=user_config)
    except OSError as e:
        if e.errno != errno.ENOENT:
            raise

    return config
