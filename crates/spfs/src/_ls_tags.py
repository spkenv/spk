from typing import Iterable

from ._config import get_config


def ls_tags(path: str = "/") -> Iterable[str]:
    """List tags and tag directories based on a tag path.

    For example, if the repo contains the following tags:
        spi/stable/my_tag
        spi/stable/other_tag
        spi/latest/my_tag
    Then ls_tags("spi") would return:
        stable
        latest
    """

    config = get_config()
    repo = config.get_repository()

    return repo.tags.ls_tags(path)
