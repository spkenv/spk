fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure().compile(&["src/server/proto/service.proto"], &["src/server/proto"])?;
    Ok(())
}
