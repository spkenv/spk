# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

"""
The setup.py file is a command line application built with setuptools.

It can be executed directly with python: `python setup.py --help`

The call to setuptools.setup (below) describes this python package and
enables all of the build, package, dist, install functionality required
to package this code for all the standard python tools like pip and pipenv
"""
from setuptools import setup, find_packages


setup(
    name="python-example",
    description="An example spk package written in python",
    version="0.1.0",
    packages=find_packages(),
    install_requires=[
        # list the python packages that this one depends on,
        # they will be pip-installed along with this one
    ],
    # Enable this line to add a command line entry
    # point for your package
    # entry_points={"console_scripts": ["python-example=python-example.cli:main"]},
)
