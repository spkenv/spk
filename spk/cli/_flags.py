# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from spk.exec import _LOGGER
from typing import Dict, List, Tuple
import os
import re
import sys
import glob
import argparse
from collections import OrderedDict

from colorama import Fore
from ruamel import yaml
import spk
import spkrs

OPTION_VAR_RE = re.compile(r"^SPK_OPT_([\w\.]+)$")


def add_runtime_flags(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--no-runtime",
        "-nr",
        action="store_true",
        help="Reconfigure the current spfs runtime (useful for speed and debugging)",
    )
    parser.add_argument(
        "--env-name",
        type=str,
        default=None,
        help="A name to use for the created spfs runtime (useful for rejoining it later)",
    )


def ensure_active_runtime(args: argparse.Namespace) -> spkrs.Runtime:

    if args.no_runtime:
        return spkrs.active_runtime()

    cmd = sys.argv
    cmd_index = cmd.index(args.command)
    cmd.insert(cmd_index + 1, "--no-runtime")
    name_args = ["--name", args.env_name] if args.env_name else []
    cmd = ["spfs", "run", *name_args, "-", "--"] + cmd
    os.execvp(cmd[0], cmd)


def add_solver_flags(parser: argparse.ArgumentParser) -> None:

    add_option_flags(parser)
    add_repo_flags(parser)
    parser.add_argument(
        "--binary-only",
        action="store_true",
        default=False,
        help="If true, never build packages from source if needed",
    )


def get_solver_from_flags(args: argparse.Namespace) -> spk.Solver:

    options = get_options_from_flags(args)
    solver = spk.Solver()
    solver.update_options(options)
    configure_solver_with_repo_flags(args, solver)
    solver.set_binary_only(args.binary_only)
    return solver


def add_option_flags(parser: argparse.ArgumentParser) -> None:

    parser.add_argument(
        "--opt",
        "-o",
        type=str,
        default=[],
        action="append",
        help="Specify build options",
    )
    parser.add_argument(
        "--no-host",
        action="store_true",
        help="Do not add the default options for the current host system",
    )


def get_options_from_flags(args: argparse.Namespace) -> spk.api.OptionMap:

    if args.no_host:
        opts = spk.api.OptionMap()
    else:
        opts = spk.api.host_options()

    for pair in getattr(args, "opt", []):

        pair = pair.strip()
        if pair.startswith("{"):
            opts.update(yaml.safe_load(pair) or {})
            continue

        if "=" in pair:
            name, value = pair.split("=", 1)
        elif ":" in pair:
            name, value = pair.split(":", 1)
        else:
            raise ValueError(
                f"Invalid option: -o {pair} (should be in the form name=value)"
            )
        opts[name] = value

    return opts


def add_request_flags(parser: argparse.ArgumentParser) -> None:

    parser.add_argument(
        "--pre",
        action="store_true",
        help="If set, allow pre-releases for all command line package requests",
    )


def parse_idents(*packages: str) -> List[spk.api.Ident]:

    idents = []
    for package in packages:
        if "@" in package:
            spec, _, stage = parse_stage_specifier(package)

            if stage == "source":
                ident = spec.pkg.with_build(spk.api.SRC)
                idents.append(ident)

            else:
                print(
                    f"Unsupported stage '{stage}', can only be empty or 'source' in this context"
                )
                sys.exit(1)

        if os.path.isfile(package):
            spec, _ = find_package_spec(package)
            idents.append(spec.pkg)

        else:
            idents.append(spk.api.parse_ident(package))

    return idents


def parse_requests_using_flags(
    args: argparse.Namespace, *requests: str
) -> List[spk.api.Request]:

    options = get_options_from_flags(args)

    out: List[spk.api.Request] = []
    for r in requests:

        if "@" in r:
            spec, _, stage = parse_stage_specifier(r)

            if stage == "source":
                ident = spec.pkg.with_build(spk.api.SRC)
                out.append(spk.api.PkgRequest.from_ident(ident))

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

            req = spk.api.PkgRequest.from_dict(request_data)
            req.required_compat = spk.api.CompatRule.API
            out.append(req)

    return out


def parse_stage_specifier(specifier: str) -> Tuple[spk.api.Spec, str, str]:
    """Returns the spec, filename and stage for the given specifier."""

    if "@" not in specifier:
        raise ValueError(
            f"Package stage '{specifier}' must contain an '@' character (eg: @build, my-pkg@install)"
        )

    package, stage = specifier.split("@", 1)
    spec, filename = find_package_spec(package)
    return spec, filename, stage


def find_package_spec(package: str) -> Tuple[spk.api.Spec, str]:

    packages = glob.glob("*.spk.yaml")
    if not package:
        if len(packages) == 1:
            package = packages[0]
        elif len(packages) > 1:
            print(
                f"{Fore.RED}Multiple package specs in current directory{Fore.RESET}",
                file=sys.stderr,
            )
            print(
                f"{Fore.RED} > please specify a package name or filepath{Fore.RESET}",
                file=sys.stderr,
            )
            sys.exit(1)
        else:
            print(
                f"{Fore.RED}No package specs found in current directory{Fore.RESET}",
                file=sys.stderr,
            )
            print(
                f"{Fore.RED} > please specify a filepath{Fore.RESET}", file=sys.stderr
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
    return spec, package


def add_repo_flags(
    parser: argparse.ArgumentParser,
    defaults: List[str] = ["origin"],
    default_local: bool = False,
    defaults_if_local: bool = True,
) -> None:

    if not default_local:
        parser.add_argument(
            "--local-repo",
            "-l",
            action="store_true",
            help="Enable resolving packages from the local repository",
        )
    else:
        parser.add_argument(
            "--no-local-repo",
            "-nl",
            dest="local_repo",
            action="store_false",
            default=True,
            help="Disable resolving packages from the local repository",
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
    parser.add_argument(
        "--disable-repo",
        "-dr",
        type=str,
        metavar="NAME",
        action="append",
        default=[],
        help="Repositories to exclude in the resolve. Any configured spfs repository can be named here",
    )


def configure_solver_with_repo_flags(
    args: argparse.Namespace, solver: spk.Solver
) -> None:

    for name, repo in get_repos_from_repo_flags(args).items():
        _LOGGER.debug("using repository", repo=name)
        solver.add_repository(repo)


def get_repos_from_repo_flags(
    args: argparse.Namespace,
) -> Dict[str, spk.storage.Repository]:

    repos: Dict[str, spk.storage.Repository] = OrderedDict()
    if args.local_repo:
        repos["local"] = spk.storage.local_repository()
    for name in args.enable_repo:
        if name not in args.disable_repo:
            repos[name] = spk.storage.remote_repository(name)
    return repos
