from typing import Iterator, Optional, Pattern, Mapping
import re
import signal
from contextlib import contextmanager

from .. import api


def data_path(pkg: api.Ident = None, prefix: str = "/spfs") -> str:
    return f"{prefix}/spk/pkg/{pkg or ''}"


_var_expansion_regex: Optional[Pattern] = None


def expand_defined_vars(value: str, vars: Mapping[str, str]) -> str:
    """Expand variables in 'value' with 'vars'.

    Expansions should be in the form of $var and ${var}.
    Undefined variables are left unchanged.
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
        if name not in vars:
            i = j
            continue
        var = vars[name]
        tail = value[j:]
        value = value[:i] + var
        i = len(value)
        value += tail
    return value


def expand_vars(value: str, vars: Mapping[str, str]) -> str:
    """Expand variables in 'value' with 'vars'.

    Expansions should be in the form of $var and ${var}.
    Unknown variables raise a KeyError.
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
        var = vars[name]
        tail = value[j:]
        value = value[:i] + var
        i = len(value)
        value += tail
    return value


@contextmanager
def deferred_signals() -> Iterator[None]:

    # do not react to os signals while the subprocess is running,
    # these should be handled by the underlying process instead
    signal.signal(signal.SIGINT, lambda *_: None)
    signal.signal(signal.SIGTERM, lambda *_: None)
    try:
        yield None
    finally:
        signal.signal(signal.SIGINT, signal.SIG_DFL)
        signal.signal(signal.SIGTERM, signal.SIG_DFL)
