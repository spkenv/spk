from typing import Callable, Any, List
import argparse
import os
import sys
import termios

import spfs
import structlog
from ruamel import yaml
from colorama import Fore, Style

import spk
from spk.io import format_ident

from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    install_cmd = sub_parsers.add_parser(
        "install", aliases=["i"], help=_install.__doc__, **parser_args,
    )
    install_cmd.add_argument(
        "packages", metavar="PKG", nargs="+", help="The packages to install",
    )
    install_cmd.add_argument(
        "--save",
        nargs="?",
        const="default",
        metavar="NAME",
        help="Also save this requirement to the local environment file",
    )
    install_cmd.add_argument(
        "--yes",
        "-y",
        action="store_true",
        default=False,
        help="Do not prompt for confirmation, just continue",
    )
    _flags.add_repo_flags(install_cmd)
    _flags.add_request_flags(install_cmd)
    install_cmd.set_defaults(func=_install)
    return install_cmd


def _install(args: argparse.Namespace) -> None:
    """install a package into spfs."""

    options = spk.api.host_options()
    solver = spk.Solver(options)
    _flags.configure_solver_with_repo_flags(args, solver)
    requests = _flags.parse_requests_using_flags(args, *args.packages)

    try:
        env = spk.current_env()
    except spk.NoEnvironmentError:
        if args.save:
            _LOGGER.warning("no current environment to install to")
            return
        raise

    if args.save:
        _append_requests_to_environment(requests, args.save)

    for solved in env.items():
        solver.decision_tree.root.force_set_resolved(
            solved.request, solved.spec, solved.repo
        )
    for request in requests:
        solver.add_request(request)

    try:
        packages = solver.solve()
    except spk.SolverError as e:
        print(f"{Fore.RED}{e}{Fore.RESET}")
        if args.verbose:
            print(spk.io.format_decision_tree(solver.decision_tree))
        else:
            print(f"{Fore.YELLOW}{Style.DIM}try '--verbose' for more info{Fore.RESET}")
        sys.exit(1)

    if not packages:
        print(f"Nothing to do.")
        return

    print("The following packages will be modified:\n")
    requested = solver.decision_tree.root.get_requests()
    primary, tertiary = [], []
    for req, spec, _ in packages.items():
        if spec.pkg.name in requested:
            primary.append(spec)
            continue
        if req not in env:
            tertiary.append(spec)

    print("  Requested:")
    for spec in primary:
        print("    " + format_ident(spec.pkg))
    if tertiary:
        print("\n  Dependencies:")
        for spec in tertiary:
            print("    " + format_ident(spec.pkg))

    print("")

    if args.yes:
        pass
    elif input("Do you want to continue? [y/N]: ").lower() not in ("y", "yes"):
        print("Installation cancelled")
        sys.exit(1)

    spk.setup_current_runtime(packages)


def _append_requests_to_environment(
    requests: List[spk.api.Request], env_name: str
) -> None:

    with open(".spk-env.yaml", "w+") as reader:
        data = yaml.round_trip_load(reader) or {}
    data.setdefault("environments", [])
    for env in data["environments"]:
        if env.get("env") == env_name:
            break
    else:
        env = spk.api.Env(env_name, requests).to_dict()
        data["environments"].append(env)

    env.setdefault("requirements", [])
    requirements = env["requirements"]
    for request in requests:
        for i, r in enumerate(requirements):
            requirement = spk.api.Request.from_dict(r)
            if requirement.pkg.name == request.pkg.name:
                _LOGGER.info(
                    "updating existing request", pkg=str(request.pkg), env=env_name
                )
                requirements[i] = request.to_dict()
                break
        else:
            _LOGGER.info("adding new request", pkg=request.pkg, env=env_name)
            requirements.append(request.to_dict())

    with open(".spk-env.yaml", "w") as writer:
        yaml.round_trip_dump(data, writer)
    _LOGGER.info("Updated: .spk-env.yaml")
