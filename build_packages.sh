set -ex

pushd packages/gcc
spk build gcc48.spk.yaml
spk build gcc63.spk.yaml
popd

pushd examples/cmake
spk build example.spk.yaml
popd

pushd packages/python
spk build python2.spk.yaml
spk build python3.spk.yaml
popd
