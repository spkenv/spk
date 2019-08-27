from setuptools import setup, find_packages

setup(
    name="spenv",
    packages=find_packages(),
    entry_points={"console_scripts": ["spenv=spenv.cli:main"]},
)
