from typing import List

import spfs

from ._option_map import OptionMap
from ._spec import Spec
from ._handle import Handle, SpFSHandle


class Solver:
    def __init__(self, options: OptionMap) -> None:

        self._options = options
        self._requests: List[Spec] = []

    def add_request(self, spec: Spec) -> None:

        self._requests.append(spec)

    def solve(self) -> List[Handle]:

        handles: List[Handle] = []
        for request in self._requests:

            # TODO: what if the request already has a release?

            all_versions = sorted(spfs.ls_tags(f"spm/pkg/{request.pkg.name}"))
            versions = list(filter(request.pkg.version.is_satisfied_by, all_versions))
            versions.sort()

            if not versions:
                raise ValueError(
                    f"unsatisfiable request: {request.pkg} from versions [{', '.join(all_versions)}]"
                )

            tag = f"spm/pkg/{request.pkg.name}/{versions[-1]}"
            # try:
            #     tag = expand_vars(tag, self._options)
            # except KeyError as e:
            #     raise ValueError(f"Expansion of undefined option '{e}'")

            handles.append(SpFSHandle(request, tag))

        return handles
