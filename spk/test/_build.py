from .. import api

class PackageBuildTester:
    def __init__(self, spec: api.Spec) -> None:
        self._spec = spec

    def test(self) -> None:
        print("test build")
