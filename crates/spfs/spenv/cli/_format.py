from colorama import Fore

import spenv

config = spenv.get_config()
repo = config.get_repository()


def format_digest(ref: str) -> str:

    aliases = repo.find_aliases(ref)
    return " -> ".join([ref] + aliases)
