from typing import Optional, List, NamedTuple, Tuple
import os
import errno


class Mount(NamedTuple):

    target: str
    ref: str
    lowerdirs: Tuple[str]
    upperdir: str
    workdir: str

    @property
    def command(self) -> Tuple[str]:

        return (
            "fuse-overlayfs",
            "-o",
            f"lowerdir={':'.join(self.lowedirs)},upperdir={self.upperdir},workdir={self.workdir}",
            self.target,
        )


class MountStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)
