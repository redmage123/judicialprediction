use std::path::PathBuf;

fn main() {
    // Root of the protos/ tree relative to this crate's manifest directory.
    // Cargo sets CARGO_MANIFEST_DIR at build time.
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let proto_root = manifest_dir
        .parent() // rust/
        .unwrap()
        .parent() // judicialpredict/
        .unwrap()
        .join("protos");

    let feature_store_proto = proto_root
        .join("judicialpredict/data_plane/feature_store/v1/feature_store.proto");
    let inference_proto = proto_root
        .join("judicialpredict/ml_plane/inference/v1/inference.proto");

    // Re-run if any proto file changes.
    println!(
        "cargo:rerun-if-changed={}",
        feature_store_proto.display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        inference_proto.display()
    );

    tonic_build::configure()
        // Emit both server and client stubs for every service.
        .build_server(true)
        .build_client(true)
        // Place generated files in OUT_DIR (default); we include! them in lib.rs.
        .compile_protos(
            &[&feature_store_proto, &inference_proto],
            // Include root so `package judicialpredict.data_plane...` resolves.
            &[&proto_root],
        )
        .unwrap_or_else(|e| panic!("tonic_build failed: {e}"));
}
