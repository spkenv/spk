// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "protobuf-src")]
    unsafe {
        std::env::set_var("PROTOC", protobuf_src::protoc());
    }
    tonic_build::configure().compile_protos(&["src/proto/defs/vfs.proto"], &["src/proto/defs"])?;
    Ok(())
}
