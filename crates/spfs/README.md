# spenv

## Development

For local development, some tests will require the privileged binary to be built and have its capabilities set. You can rely on the system install of spenv for this in most cases, or run the `build.sh` script with sudo if you need to validate changes to the `spenv-enter` binary itself.

`./build_rpm.sh` is the most consistent way to build the rpm file, which can easily be `sudo yum install`'d into the current system for validation.

The `build.sh` script compiles the binaries and python pex file for faster iteration.

For python development, however, `pipenv shell` followed by calls to `pytest` and `python -m spenv ...` are the simplest and fastest.``
