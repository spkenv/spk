fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure().compile(
        &[
            "src/server/proto/repository.proto",
            "src/server/proto/tag.proto",
        ],
        &["src/server/proto"],
    )?;
    Ok(())
}
