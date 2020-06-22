set -ex

pushd examples/cmake
spk build example.spk.yaml
popd

pushd examples/gcc
spk build gcc48.spk.yaml
spk build gcc63.spk.yaml
popd

pushd examples/python
spk build python2.spk.yaml
spk build python3.spk.yaml
popd
