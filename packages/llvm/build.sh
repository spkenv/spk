#!/usr/bin/env bash

# Note: This script should build for either Rez or spk or neither, it tries
# to decide based on the presence of the Rez or Spk env variables.

# Terminate the script on error
set -e

echo "Running custom build script for LLVM"
echo "Environment contains:"
env | sort

if [[ "${REZ_BUILD_ENV}" != "" ]] ; then
    BUILD_SCHEME=rez
    INSTALL_PREFIX=${REZ_BUILD_INSTALL_PATH}
    SOURCE_DIR="${REZ_BUILD_SOURCE_PATH}/llvm-project/llvm"
    BUILD_DIR=.
elif [[ "${SPK_PKG_NAME}" != "" ]] ; then
    BUILD_SCHEME=spk
    INSTALL_PREFIX=/spfs
    SOURCE_DIR="./llvm-project/llvm"
    BUILD_DIR=build
else
    BUILD_SCHEME=none
    : ${INSTALL_PREFIX:=_dist}
    SOURCE_DIR=./llvm-project/llvm
    BUILD_DIR=build
    echo "Build type doesn't seem to be either spk or rez"
    # exit 1
fi
echo "Building scheme: ${BUILD_SCHEME}"

# SPI magic: build with an older clang in a known location. If this is not
# found (for example, at other sites), it will just fall back on using the
# default gcc it finds.
: ${CLANGHOME:=/shots/spi/home/software/packages/llvm/11.0.0/gcc-6.3}
if [ -d $CLANGHOME ] ; then
    BUILD_WITH_CLANG_FLAGS+=" -DCMAKE_C_COMPILER=${CLANGHOME}/bin/clang"
    BUILD_WITH_CLANG_FLAGS+=" -DCMAKE_CXX_COMPILER=${CLANGHOME}/bin/clang++"
    BUILD_WITH_CLANG_FLAGS+=" -DLLVM_ENABLE_LLD=ON"
fi

TARGETS="host;NVPTX"
SANATIZERS="Address;Memory;MemoryWithOrigins;Undefined;Thread;DataFlow"
PROJECTS="clang;libcxx;libcxxabi;libunwind;compiler-rt;lld"

# requires CUDA_TOOLKIT_ROOT_DIR for OpenMP+CUDA
# Also need to be explicit about the libcuda otherwise version in /usr/lib is used.
#
# Comment this out -- this should be set by the 'cuda' dependency for either
# spk or Rez.
# export CUDA_TOOLKIT_ROOT_DIR="/shots/spi/home/lib/arnold/rhel7/cuda_11.1"

# Additions for OpenMP
PROJECTS="${PROJECTS};openmp"
OMP_CUDA_FLAGS=" -DCUDA_TOOLKIT_ROOT_DIR=${CUDA_TOOLKIT_ROOT_DIR}"
OMP_CUDA_FLAGS+=" -DLIBOMPTARGET_DEP_CUDA_DRIVER_LIBRARIES=${CUDA_TOOLKIT_ROOT_DIR}/targets/x86_64-linux/lib/stubs/libcuda.so"
OMP_CUDA_FLAGS+=" -DLIBOMPTARGET_DEP_CUDA_INCLUDE_DIRS=${CUDA_TOOLKIT_ROOT_DIR}/targets/x86_64-linux/include"
OMP_CUDA_FLAGS+=" -DLIBOMPTARGET_NVPTX_COMPUTE_CAPABILITIES=60,61,62,70,72,75,80"
OMP_CUDA_FLAGS+=" -DCLANG_OPENMP_NVPTX_DEFAULT_ARCH=sm_61" # Default to P6000

# LLDB doesn't like the swig version here, though LLDB says Swig v2+ should work
#
# PROJECTS="${PROJECTS};lldb"

cmake -S "${SOURCE_DIR}" -B "${BUILD_DIR}"                \
      -DCMAKE_BUILD_TYPE:STRING=Release -G Ninja          \
      -DCMAKE_INSTALL_PREFIX="${INSTALL_PREFIX}"          \
      -DCMAKE_CXX_STANDARD=14                             \
      -DCMAKE_CXX_STANDARD_REQUIRED=ON                    \
      -DCMAKE_CXX_EXTENSIONS=OFF                          \
      -DGCC_INSTALL_PREFIX=/opt/rh/devtoolset-6/root/usr  \
      -DLLVM_ENABLE_LTO=OFF                               \
      -DLIBCLANG_BUILD_STATIC=ON                          \
      -DLLVM_ENABLE_ASSERTIONS=OFF                        \
      -DLLVM_ENABLE_BACKTRACES=OFF                        \
      -DLLVM_TARGETS_TO_BUILD="${TARGETS}"                \
      -DLLVM_ENABLE_TERMINFO=OFF                          \
      -DLLVM_USE_INTEL_JITEVENTS=ON                       \
      -DLLVM_APPEND_VC_REV=OFF                            \
      -DLLVM_USE_SANITIZER="${SANITIZERS}"                \
      -DLLVM_ENABLE_PROJECTS="${PROJECTS}"                \
      ${BUILD_WITH_CLANG_FLAGS}                           \
      ${OMP_CUDA_FLAGS}                                   \
      "$@"

if [[ "${REZ_BUILD_INSTALL}" -eq "1" || "${SPK_PKG_NAME}" != "" ]]; then
    cmake --build ${BUILD_DIR} --target install
else
    cmake --build ${BUILD_DIR}
fi
