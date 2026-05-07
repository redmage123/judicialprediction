# JudicialPredict GitOps — App-of-Apps

This directory contains the ArgoCD Application manifests for every JudicialPredict environment.

## Pattern

We use the **App-of-Apps** pattern: one root `Application` per environment points at a
directory of child `Application` manifests. ArgoCD syncs the root Application; it discovers
and syncs each child Application automatically.

```
gitops/
├── dev/
│   ├── applications.yaml     ← root Application (synced manually once to bootstrap)
│   ├── apps/
│   │   ├── api-gateway.yaml  ← child Application per service
│   │   └── ...
│   └── values/
│       ├── api-gateway.yaml  ← env-specific Helm value overrides
│       └── ...
├── staging/                  ← Sprint 2
└── prod/                     ← Sprint 3
```

## Bootstrapping a new cluster

```bash
# 1. Install ArgoCD into the cluster.
kubectl create namespace argocd
kubectl apply -n argocd -f https://raw.githubusercontent.com/argoproj/argo-cd/stable/manifests/install.yaml

# 2. Apply the root Application for the target environment (one-time manual step).
kubectl apply -f gitops/dev/applications.yaml

# 3. ArgoCD auto-syncs from this point on.
#    Watch progress:
argocd app list
argocd app sync judicialpredict-dev-root
```

## How child Applications are added

1. Add a new `apps/<service>.yaml` pointing at `charts/<service>/`.
2. Add a `values/<service>.yaml` with environment-specific overrides.
3. Commit and push. ArgoCD's auto-sync picks up the new child Application within 3 minutes
   (or immediately on webhook).

## Security notes

- All secrets are managed via External Secrets Operator (Sprint 2). No plaintext secrets
  in this directory.
- Network policies are enforced per-service via the chart's `networkpolicy.yaml` template.
- ArgoCD itself runs in the `argocd` namespace; RBAC scopes it to the `judicialpredict-dev`
  destination namespace only.

## Repo URL placeholder

`applications.yaml` and child `apps/*.yaml` currently use:

```
https://github.com/openclaw/judicialpredict.git
```

Replace with the actual private repo URL before applying to a real cluster.
