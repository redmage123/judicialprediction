// api-gateway build script — compiles inference.proto into a tonic gRPC client.
//
// Only the CLIENT stub is generated (build_server=false) because api-gateway
// is a consumer of the InferenceService, not a server.
//
// JP-71 (S5.4): replaces the Sprint-3 reqwest HTTP shortcut with a proper
// tonic client per v2.14 spec §7 (cross-plane calls must use gRPC).

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    // CARGO_MANIFEST_DIR = …/judicialpredict/rust/api-gateway
    // parent()           = …/judicialpredict/rust
    // parent()           = …/judicialpredict
    let proto_root = manifest_dir
        .parent()
        .ok_or("could not resolve parent of rust/")?
        .parent()
        .ok_or("could not resolve parent of judicialpredict/")?
        .join("protos");

    let inference_proto =
        proto_root.join("judicialpredict/ml_plane/inference/v1/inference.proto");

    println!("cargo:rerun-if-changed={}", inference_proto.display());

    tonic_build::configure()
        // api-gateway is a CLIENT only; do not generate server scaffolding.
        .build_server(false)
        .build_client(true)
        .compile_protos(&[&inference_proto], &[&proto_root])?;

    Ok(())
}
