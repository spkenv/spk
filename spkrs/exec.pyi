from typing import List
from . import solve, Digest


def resolve_runtime_layers(solution: solve.Solution) -> List[Digest]:
    ...

def setup_current_runtime(solution: solve.Solution) -> None:
    ...

def build_required_packages(solution: solve.Solution) -> solve.Solution:
    ...
