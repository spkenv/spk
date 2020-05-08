from typing import Optional, Pattern, Mapping
import re

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
