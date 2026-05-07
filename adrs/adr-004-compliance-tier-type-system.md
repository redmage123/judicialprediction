# ADR-004: Compliance feature-tier enforcement at the Rust type-system boundary

**Status:** Accepted
**Date:** 2026-05-07
**Author:** PM-authored from spec §5 (Compliance Framework); engineer + gigforge-legal to review
**Reviewers:** gigforge-engineer (type-system enforcement), gigforge-legal (Tier-C policy + Title VII / disparate-impact framing), gigforge-legal-assoc-1 + gigforge-legal-assoc-2 (per-jurisdiction sign-off when rule encoding lands)
**Spec references:** §5 Demographic / Personality / Compliance Framework, §5.5 Compliance Architecture, §11.6.5 SOLID (DIP)
**Plane issue:** JP-13

## Context

JudicialPredict's Title VII / disparate-impact / reputational-risk exposure (per spec §18 risks) rests primarily on the discipline that Tier-C protected-class features never enter ML / GNN / NLP / Decision predictive paths.

The exception (Tier-C is permitted as a *legal element* for Title VII / ADA / FHA / ADEA / §1981 / §1983 / ECOA causes of action) has to be authorized explicitly per call site, not bypassable by accident.

The spec calls for **compile-time enforcement** via Rust ADTs (§5.5, §8.5). This ADR specifies the type-system mechanism in detail so the Rust feature-store + every downstream Rust caller has a binding contract.

## Decision

### Newtype wrappers over feature payloads

Every feature value flowing through the Rust data plane is wrapped in a typed envelope:

```rust
pub struct Feature<Tier, Sensitivity> {
    name: FeatureName,
    value: FeatureValue,
    lineage: FeatureLineage,
    _phantom: PhantomData<(Tier, Sensitivity)>,
}

// Tier marker types (zero-sized, compile-time only)
pub struct TierA;  // Judges
pub struct TierB;  // Attorneys
pub struct TierC;  // Parties (protected-class, restricted)
pub struct TierD;  // Expert witnesses

// Sensitivity marker types
pub struct Public;
pub struct QuasiPublic;
pub struct Inferred;
pub struct Protected;
```

### PermittedUse trait — the binding contract

Every Rust function that consumes feature values takes a generic `Tier` parameter constrained by a `PermittedUse` trait:

```rust
pub trait PermittedUseInModel {}
impl PermittedUseInModel for TierA {}
impl PermittedUseInModel for TierB {}
impl PermittedUseInModel for TierD {}
// TierC explicitly NOT impl — Tier-C cannot be passed to model code

pub fn extract_features_for_model<T: PermittedUseInModel>(
    case: &Case,
    features: &[Feature<T, _>]
) -> Result<ModelInputVector, FeatureStoreError> { ... }
```

A call like `extract_features_for_model(case, tier_c_features)` is a **compile error**, not a runtime check. The borrow checker enforces tier compliance.

### Element-required Tier-C exception (the narrow path)

For Title VII / ADA / FHA / ADEA / §1981 / §1983 / ECOA, the rule engine needs to read protected-class status as a legal *element* of the claim. This is permitted via an explicit token type:

```rust
pub struct ProtectedClassElementToken {
    cause_of_action: CauseOfAction,
    statutory_citation: StatutoryCitation,
    issued_at: Timestamp,
    issued_by: SubjectId,
}

// Only the rule engine can request a Tier-C feature, and only with a token
pub fn read_protected_class_for_element(
    feature: Feature<TierC, _>,
    token: ProtectedClassElementToken,
) -> ProtectedClassValue { ... }
```

The token is issued by the rule engine when it determines the cause of action requires the element. Issuance is logged with the statutory citation for audit. ML / GNN / NLP code cannot construct a `ProtectedClassElementToken` because the constructor is private to the rule-engine crate.

### Cross-tenant token (per ADR-003)

Cross-tenant feature reads use a parallel token type:

```rust
pub struct CrossTenantAuthorizedToken {
    requesting_tenant: TenantId,
    target_tenant: TenantId,
    purpose: Purpose,
    issued_at: Timestamp,
    issued_by: SubjectId,  // platform-admin only
}
```

Token construction is restricted to the platform-admin crate; every issuance is audit-logged.

### Sensitivity tags + permitted_uses metadata

Every feature is registered in the feature-store metadata table with:
- `entity_tier` (A / B / C / D)
- `sensitivity` (public / quasi-public / inferred / protected)
- `permitted_uses` — a closed set of `(plane, layer)` pairs (e.g., `(ML, Layer1)`, `(Logic, Layer2)`)
- `lineage` — source dataset, derivation chain, last update, owner
- `provenance` — full chain back to source

Adding a feature without registering metadata is a CI block (the `feature-store-types` crate exposes only registered features).

### Protected-class proxy audit

Quarterly job:
1. For every feature in production use, compute correlation with protected-class indicators on held-out validation data.
2. Features whose predictive value plausibly derives from protected-class correlation are flagged.
3. Flagged features are surfaced in the platform-admin dashboard for compliance review.
4. Reviewed features can be retired or override-approved with explicit governance.

### Per-tenant feature-tier toggles

Each firm can enable / disable specific feature categories within their tenant beyond the global rules. Some firms will want stricter posture than the default (e.g., disable attorney personality features entirely). Every toggle change is audit-logged.

### Disparate-impact reports

Quarterly per-tenant aggregate reports: outcome rates and recommendations sliced by case type and (where consensually disclosed by parties) protected class. Helps firms self-audit their pattern of practice.

### PDF memo disclosure

Every case-evaluation memo discloses:
- Which feature tiers were used.
- Tier-C usages with statutory citation justifying each.
- Federated-learning participation status for the underlying model.

The disclosure is generated mechanically from the feature-lineage data; cannot be omitted.

## Consequences

### Positive

- **Compile-time compliance.** The most expensive class of bug — Tier-C protected-class data leaking into a predictive model — is rejected at build time, not runtime. Title VII / disparate-impact exposure on the model itself is essentially zero.
- **Audit trail by construction.** Every Tier-C read carries a token logged with statutory citation. Audit reports are generated mechanically from the audit log; no hand-written reporting drift.
- **Per-tenant compliance autonomy.** Firms with stricter posture (e.g., privacy-conscious firms, firms in regulated industries) can dial up restrictions without code changes.
- **Reviewer-friendly.** A PR adding a new feature must register it in the metadata table; reviewers see exactly what tier + sensitivity + permitted-uses the new feature has, in one place.

### Negative

- **Type-system overhead** in Rust code: generic functions over `Tier` add a dimension to the type signature. Mitigated by helper macros (`feature_fn!`) that reduce boilerplate.
- **Refactor cost** when tier reclassifications happen (rare). Mitigated by the type system itself — reclassification triggers compile errors at every call site, surfacing every code path that needs review.
- **Python plane lacks the same compile-time guarantees.** Pydantic + runtime validation at the gRPC boundary is the second line of defense; the Rust feature-store is the primary line.

### Neutral / mitigations

- **Reversibility:** the type-system structure is additive; new tiers or new permitted-use traits can be added without breaking existing code (existing code continues to use the existing tiers).
- **Risk: token misuse.** A subtly-crafted code path could in principle issue a token in bad faith. Mitigated by audit-log review (any token issuance for a cause of action that doesn't actually require it is auditable), CODEOWNERS on the rule-engine crate, and Legal SME sign-off on token-issuance code paths.

## Alternatives considered

### Alternative A — Runtime filtering only (no compile-time enforcement)
**Rejected.** "Don't pass Tier-C to the model" as a documented rule, with runtime checks at the model boundary. The runtime check catches a bug after it has already shipped to a customer; compile-time prevents it from shipping at all. Given the severity rating in §18, runtime-only is too weak.

### Alternative B — Annotation / decorator-based enforcement (Python-only)
**Rejected as primary.** Python decorators can mark functions as "no Tier-C" but the enforcement is type-erasable: a runtime-typed dictionary of features can carry Tier-C without anyone noticing. We need a language whose type system cannot be erased — Rust.

### Alternative C — Capability tokens at runtime, no static types
**Rejected.** Capability-based access control at runtime is the right idea, but tokens generated and checked at runtime can be forged or misused. Static types + tokens-with-private-constructors give us both compile-time and audit-time safety.

### Alternative D — Encrypt Tier-C at rest with separate key, no type system
**Rejected.** Encryption is orthogonal — we still need encryption for storage isolation. But encryption alone doesn't prevent unauthorized in-application reads; the keys are accessible to the application by design. Defense in depth requires both.

## Compliance and verification

- **Property tests:** `feature-store-types` crate property-tests assert:
  - No combination of public APIs allows constructing a `Feature<TierC, _>` outside the rule-engine crate.
  - No `PermittedUseInModel`-bounded function can accept a `Feature<TierC, _>` (compile-time).
  - Token construction in non-rule-engine crates fails to compile.
- **CI gate:** any new feature added to the metadata table without an `entity_tier` and `permitted_uses` declaration is rejected.
- **Quarterly proxy-audit job:** runs as Argo Workflow; flagged features go to compliance review.
- **Pen test pre-pilot:** specifically targets Tier-C bypass; pilot launch gated on clean results.
- **Documentation:** `docs/compliance/tier-c-token-issuance.md` documents the approved code paths for token issuance with examples; PRs touching this surface require Compliance Engineer + Legal SME review.

## References

- `judicialpredict-v2-spec.md` §5 (Demographic / Personality / Compliance Framework)
- `judicialpredict-v2-spec.md` §5.5 (Compliance Architecture)
- `judicialpredict-v2-spec.md` §11.6.5 (SOLID per language)
- `judicialpredict-v2-spec.md` §18 (Risks — Title VII / disparate-impact rated Severe)
- ADR-001 (Polyglot architecture boundary)
- ADR-003 (Multi-tenant isolation strategy)
- ADR-FP-001 (Functional-core / imperative-shell paradigm boundaries)
- "Parse, don't validate" — Alexis King — the design philosophy behind newtype-wrapping Tier and Sensitivity at parse time rather than checking at every callsite.
- Title VII jurisprudence on disparate-impact theory.

---

*This ADR is part of the JudicialPredict architectural decision record. ADRs are append-only; supersession is documented via `Superseded by` not by edit.*
