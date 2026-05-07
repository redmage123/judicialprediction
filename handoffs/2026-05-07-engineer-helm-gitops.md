# Handoff: Helm Chart + ArgoCD Gitops Scaffold

**From:** gigforge-engineer (Chris Novak)
**To:** gigforge-pm (Jamie Okafor), gigforge-devops (Casey Muller)
**Date:** 2026-05-07 18:10 UTC
**Story:** Sprint 1, S1.8
**Plane:** JP-3

---

## What Was Built

### Helm Chart — api-gateway

| File | Purpose |
|------|---------|
| `charts/api-gateway/Chart.yaml` | Chart metadata, apiVersion v2, version 0.1.0 |
| `charts/api-gateway/values.yaml` | Default values: replicaCount=2, image, resources, probes, NetworkPolicy, env injection via Secret |
| `charts/api-gateway/templates/_helpers.tpl` | Standard Helm name/label/SA helpers |
| `charts/api-gateway/templates/deployment.yaml` | Deployment with /healthz liveness+readiness probes; DATABASE_URL from Secret; configMapRef |
| `charts/api-gateway/templates/service.yaml` | ClusterIP service, port 4000, named `http` |
| `charts/api-gateway/templates/configmap.yaml` | Empty ConfigMap in place; Sprint 2 adds env vars without chart restructure |
| `charts/api-gateway/templates/serviceaccount.yaml` | Namespaced SA, `automountServiceAccountToken: false` |
| `charts/api-gateway/templates/networkpolicy.yaml` | Ingress from tenant-facing namespaces; egress to feature-store + Postgres + DNS only |

### Dockerfile — api-gateway

`rust/api-gateway/Dockerfile`

- **Build context:** judicialpredict monorepo root (so both `rust/` and `protos/` are in scope — protos needed by `feature-store/build.rs`).
- **Build command:** `docker build -t jp-api-gateway:dev -f rust/api-gateway/Dockerfile .` (run from judicialpredict/).
- **Two-stage:** `rust:latest` builder → `debian:bookworm-slim` runner (92.7 MB final image).
- **Runtime user:** non-root UID 10001.
- **Note on cargo-chef:** cargo-chef 0.1.77 requires Rust 1.88; the `rust:latest` builder already includes 1.86+ so plain two-stage is used instead. Revisit when `rust:1.88` tag lands (~May 2026).

### ArgoCD App-of-Apps Gitops

| File | Purpose |
|------|---------|
| `gitops/README.md` | Bootstrap guide and directory conventions |
| `gitops/dev/applications.yaml` | Root Application pointing at `gitops/dev/apps/` |
| `gitops/dev/apps/api-gateway.yaml` | Child Application pointing at `charts/api-gateway/` with dev value overrides |
| `gitops/dev/values/api-gateway.yaml` | Dev overrides: replicaCount=1, RUST_LOG=debug, looser probes, smaller resources |

---

## Lint / Validation Results

| Check | Result |
|-------|--------|
| `helm lint charts/api-gateway/` | **0 failures** (INFO: icon recommended — not blocking) |
| `helm template jp-dev charts/api-gateway/ --values values.yaml` | **5 resources rendered cleanly** (NetworkPolicy, ServiceAccount, ConfigMap, Service, Deployment) |
| `yaml.safe_load` on all 5 YAML files | **5/5 valid** |
| `docker build -t jp-api-gateway:dev` | **SUCCESS** — image 92.7 MB |

---

## What Sprint 2 Needs to Do

### Per-service Helm charts (9 remaining Rust services + Python services + Django admin)
Each service needs its own `charts/<service>/` mirroring the api-gateway chart structure.
Priority order: `feature-store`, `ml-inference-svc`, `event-broker`, then the rest.

### ArgoCD Image Updater
Once GHCR is wired to the repo, install `argocd-image-updater` and annotate the api-gateway
Application so that every push to `main` that changes the binary automatically updates the
`image.tag` in the gitops values file.

### Replace placeholder repoURL
`gitops/dev/applications.yaml` and `gitops/dev/apps/api-gateway.yaml` both contain:
```
repoURL: https://github.com/openclaw/judicialpredict.git
```
Replace with the actual private repo URL (or public once the repo is created).

### NetworkPolicy tightening
- Replace `judicialpredict/role: tenant-facing` ingress selector with the real ingress-controller namespace label.
- Replace the feature-store / Postgres egress pod selectors with the exact CloudNativePG labels from the deployed chart.

### Secrets management
The Deployment expects a Kubernetes Secret named `api-gateway-db-credentials-dev` with key
`DATABASE_URL`. This must be created before the first sync (or managed via External Secrets
Operator pointing at Vault/AWS Secrets Manager).

### Cargo-chef layer cache
When Rust 1.88 lands in `rust:latest`, switch the Dockerfile back to the cargo-chef pattern
for faster incremental builds. Current two-stage build compiles all deps from scratch on
every source change.

---

## Files Created (Full List)

```
charts/api-gateway/Chart.yaml
charts/api-gateway/values.yaml
charts/api-gateway/templates/_helpers.tpl
charts/api-gateway/templates/deployment.yaml
charts/api-gateway/templates/service.yaml
charts/api-gateway/templates/configmap.yaml
charts/api-gateway/templates/serviceaccount.yaml
charts/api-gateway/templates/networkpolicy.yaml
rust/api-gateway/Dockerfile
gitops/README.md
gitops/dev/applications.yaml
gitops/dev/apps/api-gateway.yaml
gitops/dev/values/api-gateway.yaml
handoffs/2026-05-07-engineer-helm-gitops.md
```
