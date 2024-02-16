// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "protobuf-src")]
    std::env::set_var("PROTOC", protobuf_src::protoc());
    tonic_build::configure().compile(&["src/proto/defs/vfs.proto"], &["src/proto/defs"])?;
    Ok(())
}
