// Deprecated as of S4.4: results-view uses server-computed recommendation from createCase.
// This file is kept for reference; Sprint 5 removes it.
//
// MIRROR of rust/decision-arith/src/recommend.rs. Sprint-3 wave-3 follow-up: replace with a
// `recommend` GraphQL query so this duplication goes away. The Rust file is the source of truth.
//
// Decision thresholds (must match recommend.rs exactly):
//   0.40 — loss-exposure threshold for Settle branch (ci_lower < 0.40)
//   0.55 — trial-justification threshold for Try branch (ci_lower > 0.55)
//   0.40 — settlement EV anchor (EV_settle = expectedDamages × 0.40)
//
// Sprint-4 follow-up: replace the 0.40 anchor with real cost-engine + BATNA modelling (spec §5.4).

export type RecommendationKind = "Settle" | "Try" | "Borderline";

export interface Prediction {
  pWin: number;
  ciLower: number;
  ciUpper: number;
  expectedDamages: number;
}

export interface Recommendation {
  kind: RecommendationKind;
  bullets: [string, string, string];
  expectedValueTry: number;
  expectedValueSettle: number;
}

/**
 * Produce a structured recommendation from a probabilistic prediction and a litigation cost estimate.
 *
 * EV model (mirrors Rust):
 *   EV_try    = pWin × expectedDamages − cost
 *   EV_settle = expectedDamages × 0.40   (conservative US civil settlement heuristic)
 *
 * Decision rules (first match wins):
 *   1. Settle     if EV_settle > EV_try AND ciLower < 0.40
 *   2. Try        if EV_try > EV_settle AND ciLower > 0.55
 *   3. Borderline otherwise
 *
 * Bullets are deterministic: same inputs always produce identical strings.
 *
 * @param p     - probabilistic prediction output from ml-inference-svc
 * @param cost  - estimated litigation cost; defaults to $50,000 demo placeholder at call sites
 */
export function recommend(p: Prediction, cost: number): Recommendation {
  const evTry = p.pWin * p.expectedDamages - cost;
  // 0.40 anchor: US civil settlements typically land at 30–50 % of expected trial damages.
  // Using 0.40 (midpoint of empirical range) keeps the model conservative.
  const evSettle = p.expectedDamages * 0.40;

  let kind: RecommendationKind;
  if (evSettle > evTry && p.ciLower < 0.40) {
    kind = "Settle";
  } else if (evTry > evSettle && p.ciLower > 0.55) {
    kind = "Try";
  } else {
    kind = "Borderline";
  }

  // fmt mirrors Rust's Decimal::round_dp(2) Display — always two decimal places.
  const fmt = (n: number) => n.toFixed(2);

  const bullet1 = `P(win) ${fmt(p.pWin)} with 90% CI [${fmt(p.ciLower)}, ${fmt(p.ciUpper)}]`;
  const bullet2 = `Expected value at trial $${fmt(evTry)} vs. expected settlement value $${fmt(evSettle)}`;

  let bullet3: string;
  if (kind === "Settle") {
    bullet3 =
      `Settlement preferred: CI lower bound (${fmt(p.ciLower)}) is below the loss-exposure ` +
      `threshold of 0.40 and settlement EV ($${fmt(evSettle)}) exceeds trial EV ($${fmt(evTry)})`;
  } else if (kind === "Try") {
    bullet3 =
      `Trial expected value ($${fmt(evTry)}) exceeds settlement ($${fmt(evSettle)}) and lower CI bound ` +
      `(${fmt(p.ciLower)}) is above the trial-justification threshold of 0.55`;
  } else {
    // \u2013 = en dash, matching the Rust literal "0.40–0.55"
    bullet3 =
      `Outcome is borderline: CI lower bound (${fmt(p.ciLower)}) falls between thresholds ` +
      `(0.40\u20130.55) or EV comparison does not clearly favor trial or settlement`;
  }

  return {
    kind,
    bullets: [bullet1, bullet2, bullet3],
    expectedValueTry: evTry,
    expectedValueSettle: evSettle,
  };
}
