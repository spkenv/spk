from typing import Tuple
import unicodedata

_NAME_UTF_CATEGORIES = (
    "Ll",  # letter lower
    "Pd",  # punctuation dash
    "Nd",  # number digit
)
_TAG_NAME_UTF_CATEGORIES = ("Ll",)  # letter lower


def validate_name(name: str) -> str:
    """Return 'name' if it's a valide package name or raises ValueError"""

    index = _validate_source_str(name, _NAME_UTF_CATEGORIES)
    if index > -1:
        err_str = f"{name[:index]} > {name[index]} < {name[index+1:]}"
        raise ValueError(f"invalid package name at pos {index}: {err_str}")
    return name


def validate_tag_name(name: str) -> str:
    """Return 'name' if it's a valide pre/post release tag name or raises ValueError"""

    index = _validate_source_str(name, _TAG_NAME_UTF_CATEGORIES)
    if index > -1:
        err_str = f"{name[:index]} > {name[index]} < {name[index+1:]}"
        raise ValueError(f"invalid release tag name at pos {index}: {err_str}")
    return name


def _validate_source_str(source: str, valid_categories: Tuple[str, ...]) -> int:

    i = -1
    while i < len(source) - 1:
        i += 1
        char = source[i]
        category = unicodedata.category(char)
        if category in valid_categories:
            continue
        return i
    return -1
