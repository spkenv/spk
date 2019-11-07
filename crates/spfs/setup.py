from setuptools import setup, find_packages

import spenv

setup(
    name="spenv",
    version=spenv.__version__,
    packages=find_packages(),
    package_data={"spenv": ["*.sh"]},
    include_package_data=True,
    entry_points={"console_scripts": ["spenv=spenv.cli:main"]},
    zip_safe=False,
)
