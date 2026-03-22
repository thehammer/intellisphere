fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .out_dir("src/proto")
        .compile_protos(
            &["../proto/intellisphere/v1/intellisphere.proto"],
            &["../proto"],
        )?;
    Ok(())
}
