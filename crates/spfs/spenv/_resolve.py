from typing import Sequence, List, Optional, Dict, Mapping
import os
import re

from . import storage
from ._config import get_config

_var_expansion_regex = None


def resolve_runtime_envrionment(
    runtime: storage.Runtime, base: Mapping[str, str] = None
) -> Dict[str, str]:

    packages = resolve_layers_to_packages(runtime.config.layers)
    env = resolve_packages_to_environment(packages, base=base)
    env["SPENV_RUNTIME"] = runtime.rootdir
    return env


def resolve_packages_to_environment(
    packages: Sequence[storage.Package], base: Mapping[str, str] = None
) -> Dict[str, str]:

    env: Dict[str, str] = {}
    if base:
        env.update(base)

    for package in packages:
        for name, value in package.config.iter_env():
            value = _expand_vars(value, env)
            env[name] = value
    return env


def resolve_overlayfs_options(runtime: storage.Runtime) -> str:

    config = get_config()
    repo = config.get_repository()
    lowerdirs = [runtime.lowerdir]
    packages = resolve_layers_to_packages(runtime.config.layers)
    for package in packages:
        lowerdirs.append(package.diffdir)

    return f"lowerdir={':'.join(lowerdirs)},upperdir={runtime.upperdir},workdir={runtime.workdir}"


def resolve_layers_to_packages(layers: Sequence[str]) -> List[storage.Package]:

    config = get_config()
    repo = config.get_repository()
    packages = []
    for ref in layers:

        entry = repo.read_ref(ref)
        if isinstance(entry, storage.Runtime):
            raise RuntimeError(
                "runtime stack cannot include other runtimes, got:" + ref
            )
        elif isinstance(entry, storage.Package):
            packages.append(entry)
        else:
            expanded = resolve_layers_to_packages(entry.layers)
            packages.extend(expanded)
    return packages


def which(name: str) -> Optional[str]:

    search_paths = os.getenv("PATH", "").split(os.pathsep)
    for path in search_paths:
        filepath = os.path.join(path, name)
        if _is_exe(filepath):
            return filepath
    else:
        return None


def _is_exe(filepath: str) -> bool:

    return os.path.isfile(filepath) and os.access(filepath, os.X_OK)


def _expand_vars(value: str, vars: Mapping[str, str]) -> str:
    """Expand variables in 'value' with 'vars'.

    Expansions should be in the form of $var and ${var}.
    Unknown variables are replaced with an empty string.
    """
    global _var_expansion_regex

    if "$" not in value:
        return value
    if not _var_expansion_regex:
        _var_expansion_regex = re.compile(r"\$(\w+|\{[^}]*\})", re.ASCII)
    search = _var_expansion_regex.search
    start = "{"
    end = "}"
    i = 0
    while True:
        m = search(value, i)
        if not m:
            break
        i, j = m.span(0)
        name = m.group(1)
        if name.startswith(start) and name.endswith(end):
            name = name[1:-1]
        var = vars.get(name, "")
        tail = value[j:]
        value = value[:i] + var
        i = len(value)
        value += tail
    return value
