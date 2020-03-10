class UnknownReferenceError(ValueError):
    """Denotes a reference that is not known."""

    pass


class AmbiguousReferenceError(ValueError):
    """Denotes a reference that could refer to more than one object in the storage."""

    def __init__(self, ref: str) -> None:
        super(AmbiguousReferenceError, self).__init__(
            f"Ambiguous reference [too short]: {ref}"
        )
