import argparse

import spfs


def register(sub_parsers: argparse._SubParsersAction) -> None:

    tags_cmd = sub_parsers.add_parser("tags", help=_tags.__doc__)
    tags_cmd.set_defaults(func=_tags)


def _tags(args: argparse.Namespace) -> None:

    config = spfs.get_config()
    repo = config.get_repository()
    tags = repo.tags.iter_tags()
    for _, tag in tags:
        print(spfs.io.format_digest(tag.target))
