from typing_extensions import Protocol, runtime_checkable
import urllib.parse

from ._repository_file import FileRepository


class Repository(Protocol):

    pass


def open_repository(address: str) -> Repository:

    url = urllib.parse.urlparse(address)

    if url.scheme == "file":
        assert not url.hostname, "file repository cannot have hostname"
        return FileRepository(url.path)

    raise ValueError("unsupported repository scheme: " + address)
