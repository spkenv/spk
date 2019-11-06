from setuptools import setup, find_packages

setup(
    name="spenv",
    packages=find_packages(),
    package_data={'spenv': ["*.sh"]},
    include_package_data=True,
    entry_points={"console_scripts": ["spenv=spenv.cli:main"]},
)
