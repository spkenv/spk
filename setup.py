"""
The setup.py file is a command line application built with setuptools.

It can be executed directly with python: `python setup.py --help`

The call to setuptools.setup (below) describes this python package and
enables all of the build, package, dist, install functionality required
to package this code for all the standard python tools like pip and pipenv
"""
from setuptools import setup, find_packages

try:
    import configparser
except ImportError:
    import ConfigParser as configparser

install_requires = []
pipfile = configparser.ConfigParser()
pipfile.read("Pipfile")
if pipfile.has_section("packages"):
    for name, spec in pipfile.items("packages"):
        install_requires.append(name.strip("\"'") + spec.strip("\"'"))

setup(
    name="spk",
    description="The 'S' Package System: Convenience, clarity and speed.",
    version="0.12.10",
    packages=find_packages(),
    install_requires=install_requires,
    package_data={"": ["Pipfile"]},
    include_package_data=True,
    entry_points={"console_scripts": ["spk=spk.cli:main"]},
)
