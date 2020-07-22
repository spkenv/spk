from typing import Dict, List, Tuple
import sys
import glob
import argparse
from collections import OrderedDict

from colorama import Fore
from ruamel import yaml
import spk


def add_option_flags(parser: argparse.ArgumentParser) -> None:

    parser.add_argument(
        "--opt",
        "-o",
        type=str,
        default=[],
        action="append",
        help="Specify option values for the envrionment",
    )


def get_options_from_flags(args: argparse.Namespace) -> spk.api.OptionMap:

    opts = spk.api.host_options()
    for pair in getattr(args, "opt", []):

        name, value = pair.split("=")
        opts[name] = value

    return opts


def add_request_flags(parser: argparse.ArgumentParser) -> None:

    parser.add_argument(
        "--pre",
        action="store_true",
        help="If set, allow pre-releases for all command line package requests",
    )


def parse_requests_using_flags(
    args: argparse.Namespace, *requests: str
) -> List[spk.api.Request]:

    options = get_options_from_flags(args)

    out = []
    for r in requests:

        if "@" in r:
            spec, _, stage = parse_stage_specifier(r)

            if stage == "source":
                raise NotImplementedError("'source' stage is not yet supported")
            elif stage == "build":
                builder = spk.build.BinaryPackageBuilder.from_spec(spec).with_options(
                    options
                )
                for request in builder.get_build_requirements():
                    out.append(request)

            elif stage == "install":
                for request in spec.install.requirements:
                    out.append(request)
            else:
                print(
                    f"Unknown stage '{stage}', should be one of: 'source', 'build', 'install'"
                )
                sys.exit(1)
        else:
            parsed = yaml.safe_load(r)
            if isinstance(parsed, str):
                request_data = {"pkg": parsed}
            else:
                request_data = parsed

            if args.pre:
                request_data.setdefault(
                    "prereleasePolicy", spk.api.PreReleasePolicy.IncludeAll.name
                )

            out.append(spk.api.Request.from_dict(request_data))

    return out


def parse_stage_specifier(specifier: str) -> Tuple[spk.api.Spec, str, str]:
    """Returns the spec, filename and stage for the given specifier."""

    packages = glob.glob("*.spk.yaml")
    package, stage = specifier.split("@", 1)
    if not package:
        if len(packages) == 1:
            package = packages[0]
        elif len(packages) > 1:
            print(
                f"{Fore.RED}Multiple package specs in current directory{Fore.RESET}",
                file=sys.stderr,
            )
            print(
                f"{Fore.RED} > please specify a package name or filepath (eg: my-package{specifier}){Fore.RESET}",
                file=sys.stderr,
            )
            sys.exit(1)
        else:
            print(
                f"{Fore.RED}No package specs found in current directory{Fore.RESET}",
                file=sys.stderr,
            )
            print(
                f"{Fore.RED} > please specify a filepath (eg: spk/my-package.spk.yaml{specifier}){Fore.RESET}",
                file=sys.stderr,
            )
            sys.exit(1)
    try:
        spec = spk.api.read_spec_file(package)
    except FileNotFoundError:
        for filename in packages:
            spec = spk.api.read_spec_file(filename)
            if spec.pkg.name == package:
                package = filename
                break
        else:
            raise
    return spec, package, stage


def add_repo_flags(
    parser: argparse.ArgumentParser, defaults: List[str] = ["origin"]
) -> None:

    parser.add_argument(
        "--local-repo",
        "-l",
        action="store_true",
        help="Enable resolving packages from the local repository",
    )
    parser.add_argument(
        "--enable-repo",
        "-r",
        type=str,
        metavar="NAME",
        action="append",
        default=defaults,
        help="Repositories to include in the resolve. Any configured spfs repository can be named here",
    )


def configure_solver_with_repo_flags(
    args: argparse.Namespace, solver: spk.Solver
) -> None:

    for repo in get_repos_from_repo_flags(args).values():
        solver.add_repository(repo)


def get_repos_from_repo_flags(
    args: argparse.Namespace,
) -> Dict[str, spk.storage.Repository]:

    repos: Dict[str, spk.storage.Repository] = OrderedDict()
    if args.local_repo:
        repos["local"] = spk.storage.local_repository()
    for name in args.enable_repo:
        repos[name] = spk.storage.remote_repository(name)
    return repos
