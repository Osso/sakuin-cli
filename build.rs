fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .compile_protos(
            &[
                "proto/manga/v1/manga.proto",
                "proto/manga/v1/auth.proto",
                "proto/manga/v1/user.proto",
            ],
            &["proto"],
        )?;
    Ok(())
}
