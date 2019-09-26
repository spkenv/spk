from typing import Sequence, List, Optional, Dict, Mapping
import os
import re

from . import storage
from ._config import get_config

_var_expansion_regex = None


def resolve_runtime_environment(
    runtime: storage.fs.Runtime, base: Mapping[str, str] = None
) -> Dict[str, str]:

    layers = resolve_stack_to_layers(runtime.config.layers)
    env = resolve_layers_to_environment(layers, base=base)
    env["SPENV_RUNTIME"] = runtime.rootdir
    return env


def resolve_layers_to_environment(
    layers: Sequence[storage.fs.Layer], base: Mapping[str, str] = None
) -> Dict[str, str]:

    env: Dict[str, str] = {}
    if base:
        env.update(base)

    for layer in layers:
        for name, value in layer.iter_env():
            value = _expand_vars(value, env)
            env[name] = value
    return env


def resolve_overlayfs_options(runtime: storage.fs.Runtime) -> str:

    config = get_config()
    repo = config.get_repository()
    lowerdirs = [runtime.lowerdir]
    layers = resolve_stack_to_layers(runtime.config.layers)
    for layer in layers:
        rendered_dir = repo.blobs.render_manifest(layer.manifest)
        lowerdirs.append(rendered_dir)

    return f"lowerdir={':'.join(lowerdirs)},upperdir={runtime.upperdir},workdir={runtime.workdir}"


def resolve_stack_to_layers(stack: Sequence[str]) -> List[storage.fs.Layer]:

    config = get_config()
    repo = config.get_repository()
    layers = []
    for ref in stack:

        entry = repo.read_object(ref)
        if isinstance(entry, storage.fs.Layer):
            layers.append(entry)
        elif isinstance(entry, storage.fs.Platform):
            expanded = resolve_stack_to_layers(entry.layers)
            layers.extend(expanded)
        else:
            raise NotImplementedError(type(entry))
    return layers


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
