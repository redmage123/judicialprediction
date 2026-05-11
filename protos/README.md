# protos/

Protobuf source of truth for the JudicialPredict gRPC contracts (ADR-002).

## Files

| Proto file | Service | Status |
|---|---|---|
| `judicialpredict/ml_plane/inference/v1/inference.proto` | `InferenceService.PredictCaseOutcome` | Python server live (S5.3 / JP-70) |

## Python stubs (S5.3 / JP-70)

The Python gRPC server in `python/ml-inference-svc` is wired and live as of Sprint 5.
Stubs live at `python/ml-inference-svc/src/ml_inference_svc/grpc_stubs/` and are
regenerated from this directory via:

```bash
cd <project-root>
python -m grpc_tools.protoc \
  --proto_path=protos \
  --python_out=python/ml-inference-svc/src/ml_inference_svc/grpc_stubs \
  --grpc_python_out=python/ml-inference-svc/src/ml_inference_svc/grpc_stubs \
  --pyi_out=python/ml-inference-svc/src/ml_inference_svc/grpc_stubs \
  protos/judicialpredict/ml_plane/inference/v1/inference.proto
```

After regeneration, fix the absolute import in the generated `*_pb2_grpc.py`:
```
from judicialpredict... import inference_pb2
```
→ change to:
```
from ml_inference_svc.grpc_stubs.judicialpredict... import inference_pb2
```

## Rust client (JP-71)

The Rust `api-gateway` tonic client is implemented in JP-71 (Sprint 5).
Until JP-71 lands, `api-gateway` continues to call `ml-inference-svc` over HTTP `/predict`.
