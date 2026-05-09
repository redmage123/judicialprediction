/**
 * recommend.ts — pure logic tests (S3.3 / JP-44).
 *
 * Mirrors the branch coverage + stability + content tests in
 * rust/decision-arith/src/recommend.rs to verify the TypeScript port
 * produces identical decisions and bullet formatting for the same inputs.
 *
 * No rendering, no network, no external deps — pure function calls only.
 */

import { describe, it, expect } from "vitest";
import { recommend } from "../lib/recommend";

// ---------------------------------------------------------------------------
// Branch coverage — one test per RecommendationKind
// ---------------------------------------------------------------------------

describe("recommend — Settle branch", () => {
  it("returns Settle when EV_settle dominates and ci_lower < 0.40", () => {
    // Mirrors rust test: settle_when_ev_settle_dominates_and_low_ci
    // p_win=0.30, damages=$100,000, cost=$50,000
    //   EV_try    = 0.30 × 100,000 − 50,000 = −20,000
    //   EV_settle = 100,000 × 0.40           =  40,000  (> EV_try)
    //   ci_lower  = 0.30  (< 0.40) ✓ → Settle
    const rec = recommend(
      { pWin: 0.30, ciLower: 0.30, ciUpper: 0.50, expectedDamages: 100_000 },
      50_000,
    );
    expect(rec.kind).toBe("Settle");
    expect(rec.expectedValueTry).toBeCloseTo(-20_000, 4);
    expect(rec.expectedValueSettle).toBeCloseTo(40_000, 4);
  });
});

describe("recommend — Try branch", () => {
  it("returns Try when EV_try dominates and ci_lower > 0.55", () => {
    // Mirrors rust test: try_when_ev_try_dominates_and_high_ci
    // p_win=0.80, damages=$100,000, cost=$10,000
    //   EV_try    = 0.80 × 100,000 − 10,000 = 70,000  (> EV_settle)
    //   EV_settle = 100,000 × 0.40           = 40,000
    //   ci_lower  = 0.65  (> 0.55) ✓ → Try
    const rec = recommend(
      { pWin: 0.80, ciLower: 0.65, ciUpper: 0.92, expectedDamages: 100_000 },
      10_000,
    );
    expect(rec.kind).toBe("Try");
    expect(rec.expectedValueTry).toBeCloseTo(70_000, 4);
    expect(rec.expectedValueSettle).toBeCloseTo(40_000, 4);
  });
});

describe("recommend — Borderline branch", () => {
  it("returns Borderline when ci_lower is in the ambiguous zone (0.40–0.55)", () => {
    // Mirrors rust test: borderline_when_ci_is_in_ambiguous_zone
    // p_win=0.50, damages=$100,000, cost=$45,000
    //   EV_try    = 0.50 × 100,000 − 45,000 = 5,000
    //   EV_settle = 100,000 × 0.40           = 40,000  (> EV_try)
    //   ci_lower  = 0.45 — not < 0.40 (Settle fails); not > 0.55 (Try fails) → Borderline
    const rec = recommend(
      { pWin: 0.50, ciLower: 0.45, ciUpper: 0.60, expectedDamages: 100_000 },
      45_000,
    );
    expect(rec.kind).toBe("Borderline");
  });
});

// ---------------------------------------------------------------------------
// Bullet content — verify expected substrings
// ---------------------------------------------------------------------------

describe("recommend — bullet content", () => {
  it("bullet[0] contains P(win), ci_lower, ci_upper, and bullet[1] mentions EV", () => {
    // Mirrors rust test: bullet_one_contains_p_win_formatted
    // p_win=0.42 → bullet[0] must contain "P(win) 0.42", "0.31", "0.53"
    const rec = recommend(
      { pWin: 0.42, ciLower: 0.31, ciUpper: 0.53, expectedDamages: 80_000 },
      20_000,
    );
    // bullet[0]: "P(win) 0.42 with 90% CI [0.31, 0.53]"
    expect(rec.bullets[0]).toContain("P(win)");
    expect(rec.bullets[0]).toContain("0.42");
    expect(rec.bullets[0]).toContain("0.31");
    expect(rec.bullets[0]).toContain("0.53");
    // bullet[1]: "Expected value at trial $... vs. expected settlement value $..."
    expect(rec.bullets[1]).toContain("Expected value at trial");
    expect(rec.bullets[1]).toContain("expected settlement value");
  });
});

// ---------------------------------------------------------------------------
// Stability — same inputs produce identical bullet strings
// ---------------------------------------------------------------------------

describe("recommend — stability", () => {
  it("identical inputs always produce identical bullets and kind (no randomness)", () => {
    // Mirrors rust tests: bullets_stable_for_settle_variant + bullets_stable_for_try_variant
    const p = { pWin: 0.25, ciLower: 0.18, ciUpper: 0.44, expectedDamages: 250_000 };
    const cost = 80_000;

    const r1 = recommend(p, cost);
    const r2 = recommend(p, cost);

    expect(r1.kind).toBe(r2.kind);
    expect(r1.bullets[0]).toBe(r2.bullets[0]);
    expect(r1.bullets[1]).toBe(r2.bullets[1]);
    expect(r1.bullets[2]).toBe(r2.bullets[2]);
  });
});
