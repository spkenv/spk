fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure().compile(
        &[
            "src/proto/defs/database.proto",
            "src/proto/defs/repository.proto",
            "src/proto/defs/payload.proto",
            "src/proto/defs/tag.proto",
            "src/proto/defs/types.proto",
        ],
        &["src/proto/defs"],
    )?;
    Ok(())
}
