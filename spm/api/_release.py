from dataclasses import dataclass


@dataclass
class Release:
    """Release represents a package release specification."""

    source: str

    def __str__(self) -> str:

        return self.source


def parse_release(release: str) -> Release:

    # TODO: actually parse the release
    return Release(release)
