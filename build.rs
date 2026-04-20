fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=sentinel-spec/proto/sentinel/market/v1/market_data.proto");
    prost_build::compile_protos(
        &["sentinel-spec/proto/sentinel/market/v1/market_data.proto"],
        &["sentinel-spec/proto/"],
    )?;
    Ok(())
}
