# Spk Launcher

This new spk-launcher binary replaces what would normally be the "spk"
binary entrypoint for spk. It currently can behave in "spk" mode or "spawn"
mode, depending on the name of the binary. A suggested usage is to create
a symlink like:

     /usr/local/bin/spk -> spk-launcher

This is intended to allow running different versions of spk, for
example running a development version to test out new features. A user
would be expected to set `$SPK_BIN_TAG` to the name of a version. If this is
not set, then the normally installed version in /opt/spk.dist is used.

Setting `SPK_BIN_TAG=test` causes spk-launcher to check spfs for a platform
tagged like `"spk/spk-launcher/test"` and will make a local copy of that
platform's files under `/dev/shm/spk/<platform digest>`. If this path already
exists then it is assumed to already be installed and the overhead of
installing it is skipped. It will then set $SPK_BIN_PATH to
`/dev/shm/spk/<platform digest>/opt/spk.dist/spk` and exec it.

The script in `bin/create-spk-platform` is provided as a example of how to
produce a new spfs platform suitable for use with the mini-wrapper.
