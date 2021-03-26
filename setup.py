"""
The setup.py file is a command line application built with setuptools.

It can be executed directly with python: `python setup.py --help`

The call to setuptools.setup (below) describes this python package and
enables all of the build, package, dist, install functionality required
to package this code for all the standard python tools like pip and pipenv
"""
from setuptools import setup, find_packages
from setuptools_rust import Binding, RustExtension

setup(
    name="spk",
    description="The 'S' Package System: Convenience, clarity and speed.",
    version="0.26.0",
    packages=find_packages(),
    entry_points={"console_scripts": ["spk=spk.cli:main"]},
    rust_extensions=[
        RustExtension(
            "spkrs",
            binding=Binding.PyO3,
        )
    ],
)
