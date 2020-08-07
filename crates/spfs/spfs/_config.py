from typing import Optional, List
import os
import errno
import configparser

import structlog

from . import storage, runtime

_DEFAULTS = {"storage": {"root": os.path.expanduser("~/.local/share/spfs")}}
_CONFIG: Optional["Config"] = None
_LOGGER = structlog.get_logger("spfs.config")


class Config(configparser.ConfigParser):
    def __init__(self) -> None:
        super(Config, self).__init__()

    @property
    def storage_root(self) -> str:
        """Return the root path of the local repository storage."""
        return str(self["storage"]["root"])

    @property
    def runtime_storage_root(self) -> str:
        """Return the path to the local runtime storage."""
        return os.path.join(self.storage_root, "runtimes")

    def list_remote_names(self) -> List[str]:
        """List the names of all configured remote repositories."""

        names = []
        for section in self:
            if section.startswith("remote."):
                names.append(section.split(".")[1])
        return names

    def get_repository(self) -> storage.fs.FSRepository:
        """Get the local repository instance as configured."""

        try:
            return storage.fs.FSRepository(self.storage_root, create=True)
        except storage.fs.MigrationRequiredError:
            _LOGGER.warning(
                "Your local data is out of date! it will now be upgraded..."
            )
            from .storage.fs import migrations

            migrations.upgrade_repo(self.storage_root)
        return storage.fs.FSRepository(self.storage_root)

    def get_runtime_storage(self) -> runtime.Storage:
        """Get the local runtime storage, as configured."""

        return runtime.Storage(self.runtime_storage_root)

    def get_remote(self, name_or_address: str) -> storage.Repository:
        """Get a remote repostory by name or address."""

        try:
            addr = self[f"remote.{name_or_address}"]["address"]
        except KeyError:
            addr = name_or_address
        try:
            return storage.open_repository(addr)
        except Exception as e:
            raise ValueError(str(e))


def get_config() -> Config:
    """Get the current configuration, loading it if necessary."""

    global _CONFIG
    if _CONFIG is None:
        _CONFIG = load_config()
    return _CONFIG


def load_config() -> Config:
    """Load the spfs configuration from disk.

    This includes the default, user and system configurations, if they exist.
    """

    user_config = os.path.expanduser("~/.config/spfs/spfs.conf")
    system_config = "/etc/spfs.conf"

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

    try:
        config.get_repository()
        config.get_runtime_storage()
    except Exception:
        pass
    return config
