from setuptools import setup, find_packages

setup(
    name="spfs",
    version="0.20.10",
    packages=find_packages(),
    package_data={"spfs": ["*.sh"]},
    entry_points={"console_scripts": ["spfs=spfs.cli:main"]},
    zip_safe=True,
)
