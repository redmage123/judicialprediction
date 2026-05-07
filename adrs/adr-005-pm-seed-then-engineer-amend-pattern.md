# ADR-005: PM-seed-then-engineer-amend pattern for architectural decisions

**Status:** Accepted
**Date:** 2026-05-07
**Author:** gigforge-engineer (Chris Novak persona, Claude Sonnet 4.6)
**Reviewers:** gigforge-pm (Jamie Okafor), gigforge-qa (Riley Svensson)
**Spec references:** §11.6 (Engineering Methodology), §11.6.1 (Agile/Scrum), §11.6.4 (BDD — Three Amigos)
**Plane issue:** JP-2

## Context

Sprint 1 required five ADRs (ADR-001 through ADR-004 + ADR-FP-001) before a line of code could be written. The first engineer dispatch returned off-topic output (the Hetzner dev node was running a model too weak for multi-step file-I/O instruction following). Rather than block the sprint, the PM authored ADR-002, ADR-FP-001, ADR-003, and ADR-004 as seeds from the spec — then flagged them for engineer review in a subsequent sprint.

This pattern — PM seeds an ADR from the spec, engineer reviews and amends it after implementation — recurred in Sprint 1 and is likely to recur whenever:

1. Agent capacity is temporarily constrained (model degraded, rate limit, interrupted session).
2. Reflective work (writing down a decision already made) is lower priority than forward work (implementing the decision).
3. The PM has enough spec knowledge to produce a plausible seed but not enough implementation experience to validate it against code.

Without an explicit policy, PM-seeded ADRs carry unclear authority: are they binding? Who can amend them? When must the engineer review happen? This ADR defines the rules.

## Decision

**PM-seeded ADRs are provisional until engineer-reviewed. Engineer review must occur within the next sprint cycle.**

### PM-seed rules

1. PM-seeded ADRs carry a `**Author:** PM-authored from spec §X.Y; engineer to review` stamp in the header.
2. They are created with `**Status:** Accepted` — they are operationally binding immediately (the implementation proceeds against them). They are **not** Proposed pending review; the seed is the working baseline.
3. The PM may only seed from the spec. Inventing architecture not grounded in the spec is out of scope for PM seeding.
4. PM-seeded ADRs are identified in the sprint handoff as requiring engineer review in the next sprint.

### Engineer-review rules

1. The engineer reads the PM-seeded ADR and the corresponding shipped code within the next sprint after the ADR was created.
2. The engineer appends an `## Engineer Review — YYYY-MM-DD` section at the end of the ADR (append-only; never edits existing content).
3. The review section must cover:
   - Aspects of the seed that match shipped reality (cite specific files and line ranges).
   - Aspects where reality diverged from the seed (with specific code references and an explanation of why).
   - Amendments required: if minor, add an `### Amendment — YYYY-MM-DD` subsection at the end of the review. If substantial (design disagreement, not just an execution gap), propose a new superseding ADR.
   - Review stamp: `**Reviewed by:** gigforge-engineer (<persona>, <model>)`.
4. If the engineer finds a critical design flaw — not just an execution gap — they raise it immediately as a blocker, not at sprint-end review.

### When PM-seeding is appropriate

| Situation | PM-seed appropriate? | Notes |
|-----------|---------------------|-------|
| Agent capacity constrained (model weak / rate-limited) | ✅ Yes | Standard fallback. |
| Reflective ADR for a decision already made by the team | ✅ Yes | PM writes what the team decided. |
| ADR for a code-design decision requiring implementation insight | ⚠️ Conditional | PM may seed the context + decision statement; engineer must fill in the consequences + alternatives in the review. |
| Performance-critical algorithm selection | ❌ No | Engineer must author these; PM lacks the implementation context to seed meaningfully. |
| Security / compliance architectural decisions | ⚠️ Conditional | PM may seed from spec/legal requirements; engineer + legal SME must co-review before any code ships. |
| Decisions involving external vendor / protocol selection | ✅ Yes | PM can seed from research; engineer validates against actual tooling in the review. |

### Versioning and audit

- PM-seeded ADRs are part of the append-only ADR record. The seed date and PM-authored stamp are permanent.
- Engineer reviews are also append-only. An engineer who disagrees with the seed must propose a new ADR, not edit the seed.
- The review must be committed to the repo before the sprint retrospective closes.

## Consequences

### Positive

- **Forward progress without blocking on agent capacity.** PM-seeded ADRs let implementation proceed immediately; the review cycle surfaces discrepancies before they calcify.
- **Spec grounding is preserved.** PM seeding from the spec (not from implementation intuition) keeps the ADR connected to the product requirements even when the engineer is unavailable.
- **Transparent authority.** Every participant knows whether an ADR has been engineer-validated. The `PM-authored; engineer to review` stamp is visible in the header; the engineer review section is visible at the bottom.
- **Execution gaps surface quickly.** The review cadence (within one sprint) means gaps between ADR intent and shipped code are documented while the implementation is fresh, not months later.

### Negative

- **PM-seeded ADRs may over-specify implementation details the engineer would not have chosen.** The engineer review is the correction mechanism, but it creates rework if the PM seed is significantly off. Mitigated by keeping seeds high-level (what, not how) and by the Three Amigos session before implementation (§11.6.4).
- **Two-pass cost.** Writing the seed + writing the review is more total work than writing one well-informed ADR from scratch. The tradeoff is worth it when the alternative is a blocked sprint.
- **Risk of review debt.** If engineer reviews are deprioritised across multiple sprints, PM-seeded ADRs accumulate without validation. Mitigated by the sprint-review gate: the retrospective is blocked until all ADR reviews from the preceding sprint are committed.

### Neutral / mitigations

- **Reversibility:** this process pattern has no lock-in. If the team finds a better workflow (e.g., async-collaborative ADR authoring), they adopt it and supersede this ADR.
- **Scope of amendment:** minor divergences (naming, path conventions, execution gaps) are amended in-place in the review section. Substantive disagreements (wrong design decision, wrong algorithm, security flaw) trigger a new superseding ADR — they are never silently fixed.

## Alternatives considered

### Alternative A — Block sprint progress until a qualified agent can author the ADR

**Rejected.** Sprint 1 had ADRs blocked for ~24h due to model capacity. Blocking implementation on ADR authorship adds calendar cost that compounds across multiple blocked sprints. The PM-seed-then-review pattern has a clear engineer correction mechanism; pure blocking does not.

### Alternative B — Skip ADRs when capacity is constrained; retrofit them post-sprint

**Rejected.** Retrofitted ADRs are typically rationalizations of existing code, not genuine decision records. The PM seed, even if imperfect, creates a baseline that the engineer reviews against real implementation — that is much more valuable than a post-hoc justification.

### Alternative C — PM authors context + decision; leave consequences + alternatives blank for engineer to fill

**Considered.** This is a valid alternative for code-design ADRs where the PM truly cannot fill in the consequences. In practice the Sprint 1 PM seeds (ADR-002 through ADR-FP-001 through ADR-004) were complete enough that leaving sections blank would have created confusion. The append-only Engineer Review section achieves the same goal: the engineer adds what the PM missed, without losing the PM's grounding from the spec.

### Alternative D — Use a shared authoring session (PM + engineer simultaneously)

**Rejected for now.** Requires both agents to be available at the same time in the same session context. Not feasible with the current async multi-agent workflow. May become viable if synchronous agent sessions are supported in future infrastructure.

## Compliance and verification

- **Sprint retrospective gate:** the sprint retro is blocked until all ADR reviews from that sprint cycle are committed to `adrs/`. This is enforced by the PM as a sprint completion criterion.
- **Header stamp check:** a CI lint (to be wired in Sprint 2) will verify that ADR files with `PM-authored` in the Author field also have an `## Engineer Review` section. PRs that add or modify ADRs without satisfying this structure will be flagged.
- **Review SLA:** PM-authored ADRs must be engineer-reviewed within one sprint cycle (≤ 2 weeks from the ADR creation date). Overdue reviews are flagged in the daily standup.
- **New engineer onboarding:** this ADR is included in the onboarding reading list alongside ADR-001 and ADR-FP-001 so that every new team member understands the review obligation.

## References

- `judicialpredict-v2-spec.md` §11.6 (Engineering Methodology)
- `judicialpredict-v2-spec.md` §11.6.1 (Agile/Scrum — sprint completion criteria)
- `judicialpredict-v2-spec.md` §11.6.4 (BDD — Three Amigos)
- ADR-001 (Polyglot architecture boundary — first PM-seeded ADR, engineer-authored as a clean pass)
- ADR-002 (gRPC contracts — PM-seeded; engineer review appended 2026-05-07)
- ADR-FP-001 (Functional-core / imperative-shell — PM-seeded; engineer review appended 2026-05-07)
- ADR-003 (Multi-tenant isolation — PM-seeded; engineer review appended 2026-05-07)
- ADR-004 (Compliance tier type system — PM-seeded; engineer review appended 2026-05-07)
- Michael Nygard, "Documenting Architecture Decisions" (2011) — original ADR format.

---

*This ADR is part of the JudicialPredict architectural decision record. ADRs are append-only; supersession is documented via `Superseded by` not by edit.*
