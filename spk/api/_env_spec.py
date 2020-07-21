from typing import List, Any, Dict, Union, IO, Iterable
from dataclasses import dataclass, field
import os

import structlog
from ruamel import yaml

from ._request import Request


_LOGGER = structlog.get_logger("spk")


@dataclass
class Env:

    name: str = "default"
    requirements: List[Request] = field(default_factory=list)

    def extend(self, other: "Env") -> None:

        self.name = other.name
        self.requirements.extend(other.requirements)

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "Env":

        name = data.pop("env")

        env = Env(name)
        for req in data.pop("requirements", []):
            env.requirements.append(Request.from_dict(req))

        if len(data):
            raise ValueError(f"unrecognized fields in env: {', '.join(data.keys())}")

        return env

    def to_dict(self) -> Dict[str, Any]:

        return {
            "env": self.name,
            "requirements": [r.to_dict() for r in self.requirements],
        }


@dataclass
class EnvSpec:

    environments: List[Env] = field(default_factory=list)

    def get_env(self, name: str) -> Env:

        merged_env = Env()
        for env in self.environments:
            merged_env.extend(env)
            if env.name == name:
                return merged_env
        else:
            raise ValueError(f"Environment not defined: '{name}'")

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "EnvSpec":

        spec = EnvSpec()
        for env in data.pop("environments", []):
            spec.environments.append(Env.from_dict(env))

        if len(data):
            raise ValueError(
                f"unrecognized fields in env spec: {', '.join(data.keys())}"
            )

        return spec

    def to_dict(self) -> Dict[str, Any]:

        return {
            "environments": list(e.to_dict() for e in self.environments),
        }


def read_env_spec_file(filepath: str) -> EnvSpec:
    """Load an environment specification from a yaml file."""

    filepath = os.path.abspath(filepath)
    with open(filepath, "r") as f:
        spec = read_env_spec(f)

    return spec


def read_env_spec(stream: IO[str]) -> EnvSpec:

    yaml_data = yaml.safe_load(stream)
    return EnvSpec.from_dict(yaml_data)
