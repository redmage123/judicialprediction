# JudicialPredict — Kubernetes Cluster Topology Proposal

**Sprint:** 1
**Plane issue:** JP-1
**Owner:** Senior SRE / Platform Engineer (NEW HIRE) + Casey Muller (DevOps)
**Status:** Draft for review by gigforge-pm + Operations Director
**Date:** 2026-05-07
**Spec reference:** §11.5 Platform — Kubernetes + GitOps

> Seeded by PM during agent-stall recovery; SRE to review, refine cost estimates, and execute. The `gigforge-devops` agent will iterate on the implementation specifics once tool-use is reliable on available models.

## 1. Cloud provider recommendation

**Primary recommendation: AWS EKS.** Secondary: GCP GKE if AWS pricing comes back unfavorable at scoping.

| Factor | EKS | GKE | AKS |
|--------|-----|-----|-----|
| GPU availability (T4 / L4 / A10) | Strong (g4dn, g5, g6) | Strong (T4, L4, A100 across regions) | Variable (NCas-T4-v3 limited regions) |
| CloudNativePG support | First-class | First-class | Workable |
| ArgoCD / Flux maturity | Strong | Strong | Strong |
| Region availability for US legal data | Excellent (us-east-1, us-west-2) | Good | Good |
| Spot / preemptible pricing for training | g6.xlarge spot ~70% off | Spot VMs ~60-70% off | Spot ~70% off |
| Data egress cost | Higher | Lower | Mid |
| Existing team familiarity | Likely (industry-default) | Less | Less |
| SOC 2 readiness story | Strong (control plane is PCI-DSS, ISO 27001) | Strong | Strong |

**Tiebreakers in EKS's favor:**
- AWS has the deepest legal-tech-customer track record; pilot firms are far more likely to have AWS BAA / DPA paperwork already, accelerating pilot onboarding.
- g6 instances (NVIDIA L4) are well-priced for our model fine-tune workload.
- EKS Fargate gives a fallback pathway for stateless services without managing nodes ourselves if SRE bandwidth becomes tight.

**Region:** `us-east-1` for the production cluster; `us-east-2` as a DR target. CA + NJ jurisdiction compliance does not require a specific data-residency region beyond the US.

**Estimated Phase-1 monthly run-rate:** $4,800–$7,200 across compute + storage + network for a single-pilot footprint, scaling to $9,000–$14,000 at full Phase-1 pilot capacity (10 pilot firms, ~50 active users). To be confirmed by SRE with actual workload modeling.

## 2. Node pool topology

### `general-pool` (CPU, autoscaled)

- **Instance type:** `m6i.xlarge` (4 vCPU, 16 GiB) baseline; can scale to `m6i.2xlarge` for higher-throughput services. Spot-eligible where workload tolerates eviction.
- **Min nodes:** 3 (multi-AZ minimum for prod).
- **Max nodes:** 12 (autoscaled by HPA on the workloads).
- **Workloads:**
  - Rust API gateway + partner gateway
  - Rust feature store / compliance enforcement
  - Rust ingest fetcher + feature deriver
  - Rust real-time event broker
  - Python `ml-inference-svc`
  - Python `llm-client-svc`
  - Python `nlp-svc`
  - Python `logic-svc`
  - Python `personality-svc`
  - Python `causal-inference-svc`
  - Django admin
  - Next.js workspace (SSR)
- **Taints:** none (default workload pool).
- **Labels:** `pool=general`.

### `gpu-pool` (GPU, batch-only)

- **Instance type:** `g6.xlarge` (4 vCPU, 16 GiB, 1× NVIDIA L4 24 GiB) for inference + light fine-tunes; `g6.2xlarge` (8 vCPU, 32 GiB, 1× L4) for heavier fine-tunes; `g6e.4xlarge` if Gemma 4 fine-tunes need more headroom. Spot-eligible.
- **Min nodes:** 0 (scale-to-zero when no training jobs queued).
- **Max nodes:** 4.
- **Workloads:**
  - `ml-training-job` Argo Workflows
  - Gemma 4 LoRA fine-tunes (`judicialpredict-en` + `personality-en`)
  - Embedding-generation batch jobs (KG + text + personality + topic vectors)
  - GNN training jobs (HGT / TGN / R-GCN / GraphSAGE)
  - Causal-inference long runs
- **Taints:** `nvidia.com/gpu=true:NoSchedule` so general workloads cannot land here.
- **Tolerations:** added explicitly to GPU jobs.
- **Labels:** `pool=gpu`, `nvidia.com/gpu.present=true`.

### `system-pool` (cluster services)

- **Instance type:** `t3.medium` × 3, multi-AZ.
- **Workloads:** ArgoCD, External Secrets Operator, cert-manager, Traefik controller pods, Prometheus / Grafana / Loki / Tempo / Alertmanager.
- **Taints:** `system=true:NoSchedule`.
- **Why separate:** keeps cluster services out of general workload contention; survives general-pool autoscaling churn.

## 3. Operator selections

### Postgres — **CloudNativePG**

- **Why:** managed failover, point-in-time recovery, automated backups to S3, declarative scaling, mature operator (CNCF Sandbox project). Battle-tested in production for compliance-sensitive workloads.
- **Alternative considered:** Stolon, Crunchy. Stolon has weaker backup story; Crunchy is excellent but commercial features behind paywall don't justify the cost at our scale.
- **Configuration:** 3-node cluster (1 primary + 2 replicas) per environment, multi-AZ. PITR retention 7 days `staging`, 30 days `prod`. Backups encrypted with KMS keys per tenant pgvector namespace.

### Neo4j — **official Neo4j Helm chart with PVCs**

- **Why:** official operator is community-tier limited; the Helm chart with managed PVCs is sufficient for Phase 1's expected graph size (~5M nodes, ~50M edges). Revisit operator for Phase 2 when graph sizes grow and HA becomes critical.
- **Alternative considered:** Memgraph (faster reads but smaller community); Neo4j AuraDB (managed but less control). Sticking with self-hosted Community Edition unless commercial features are needed.
- **Configuration:** 1-node `prod` (community edition limitation), with twice-daily backups to S3 + cross-region replication via custom CronJob.

### Redis — **Bitnami Helm chart** (not the operator)

- **Why:** the Bitnami chart is mature, well-documented, and our use case (cache + Streams pub/sub) doesn't require the operator's lifecycle complexity. Move to Redis Operator only when we need automatic shard rebalancing.
- **Configuration:** Sentinel topology (1 primary + 2 replicas + 3 sentinels), persistence enabled, AOF every 1 second.

### MinIO — **MinIO Operator**

- **Why:** S3-compatible storage for blob assets (case-fact PDFs, transcripts, raw ingestion blobs, model artifacts, federated-learning local model snapshots). Operator gives multi-tenant bucket policies and KMS integration. Alternative: AWS S3 directly — rejected for Phase 1 because of egress cost and tenant-isolation policy enforcement is cleaner with our own MinIO behind the cluster.
- **Configuration:** 4-node distributed mode (erasure-coded), 4 disks per node, ~6 TiB usable storage per environment, KMS-backed encryption.

## 4. Ingress + TLS

- **Traefik** as ingress controller — confirmed.
  - Dashboard exposed only on internal cluster network (no public access).
  - Persistence on S3 (HTTP challenges + Let's Encrypt records).
- **cert-manager** + Let's Encrypt for TLS.
  - Wildcard certs per environment (`*.dev.judicialpredict.com`, `*.staging`, `*.judicialpredict.com`).
  - Cluster-wide rate-limit on issuer to avoid LE backoff windows.
- **NetworkPolicies** (Calico CNI):
  - Customer traffic flows: `Internet → Traefik → main api-gateway → ML inference services → DBs`.
  - Cross-namespace direct access blocked by default; explicit allowlist.
  - Egress restricted: services can only reach LLM provider, partner-API endpoints, and KMS — no general internet.

## 5. Secrets management

**Primary recommendation: External Secrets Operator + AWS Secrets Manager.**

- **Why:** ESO syncs secrets from cloud KMS into K8s Secret resources at controlled refresh intervals. AWS Secrets Manager has rotation hooks for Postgres, Redis, S3 (via STS), tenant encryption keys. Audit trail in AWS CloudTrail.
- **Alternative considered:** HashiCorp Vault. Stronger feature set (dynamic creds, transit secrets engine, PKI) but adds an operational service we'd self-host. Phase 2 if/when transit-secret needs grow.
- **Configuration:**
  - One ESO ClusterSecretStore per environment.
  - Secrets refreshed every 1 hour.
  - Per-tenant encryption keys in AWS KMS, accessed via IAM-Roles-for-Service-Accounts (IRSA).
  - Sealed Secrets as an emergency fallback only (not primary).

## 6. Observability stack

- **Prometheus** for metrics. Federated query across cluster + external Prometheus instances if we ever go multi-cluster.
- **Grafana** for dashboards. Per-service SLO dashboards mandatory. Per-tenant dashboards in admin namespace.
- **Loki** for logs. Structured-log emission across both planes.
- **Tempo** for distributed traces. OpenTelemetry SDK on every service.
- **Alertmanager** → PagerDuty (production) + Slack (staging + dev). Severity-based routing.

Run on `system-pool`. Resource budget: ~8 vCPU / 32 GiB total for the observability stack across the cluster.

## 7. CI / CD topology

(Already specified in spec §11.5 — included here for completeness.)

- GitHub Actions on the source mono-repo.
- ArgoCD on cluster pulling from `gitops/` directory of the same mono-repo.
- ArgoCD App-of-Apps pattern with one root Application per environment (`dev`, `staging`, `prod`).
- `dev` auto-syncs; `staging` + `prod` manual-sync.
- Argo Rollouts canary deploys gated on Prometheus metric queries (P99 latency, error rate, conformal-coverage drift).

## 8. Pilot-time scaling plan

| Phase | Active users | Cases / day | Cluster capacity | Monthly run-rate (estimate) |
|-------|-------------|-------------|------------------|-----------------------------|
| Pre-pilot dev/staging | 8 (team) | ~5 synthetic | 1 cluster, single-AZ | $1,800 |
| Pilot launch (1 firm) | ~10 | ~5 real | 1 cluster, multi-AZ prod, single-AZ staging | $4,800–$7,200 |
| Phase-1 full pilot (10 firms) | ~50 | ~30 | Same cluster, scaled out | $9,000–$14,000 |
| Phase 2 (post-launch GA) | ~200 | ~120 | Multi-cluster region failover | TBD by Phase-2 architecture |

## 9. Open questions for SRE review

1. **Cloud provider final selection** — AWS EKS recommended; depends on pricing model from Operations and any pilot-firm BAA preferences.
2. **CNI choice** — Calico is the default in this proposal. Cilium has stronger eBPF-based observability but adds operational complexity. Default Calico unless SRE has a strong preference.
3. **Backup retention windows** — 7 days staging / 30 days prod is conservative; legal industry expects 7 years for some artifacts. Need to clarify which artifacts (raw ingestion blobs? model outputs? case workspaces?) require the long retention.
4. **Disaster recovery RTO/RPO targets** — not yet specified. Recommendation: RTO 4 hours, RPO 15 minutes for `prod`. Confirm with Operations.
5. **Multi-region strategy** — not in Phase 1. Document as a Phase-2 question now so we don't paint ourselves into a corner.

## 10. Next actions (after this proposal is accepted)

1. SRE to provision the cloud account + IAM baseline.
2. Cluster bootstrap via Terraform + Helmfile (or Crossplane if we go cloud-native infrastructure).
3. ArgoCD install + first App-of-Apps.
4. Stateful operators (CloudNativePG → Neo4j → Redis → MinIO) install in that order.
5. Traefik + cert-manager + ESO bootstrap.
6. First service deploy: a healthcheck-only Rust gateway + Python smoke service to verify the full deploy loop end-to-end.

---

*This proposal was seeded by the PM during agent-stall recovery. SRE to review, refine cost estimates with actual workload models, and produce the implementation runbook. The `gigforge-devops` agent will be re-engaged for the bootstrap implementation once tool-use is reliable on available models.*
