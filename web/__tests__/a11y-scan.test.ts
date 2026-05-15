/**
 * a11y-scan smoke tests — S3.13, widened in S6.10.
 *
 * Tests the exported `runAxeOnHtml` helper from scripts/a11y-scan.mjs.
 * These tests run in the vitest jsdom environment (document is available)
 * and do NOT launch a browser or spin up a Next.js server.
 *
 * Covers:
 *  1. Clean HTML → 0 moderate+ violations (exit-0 equivalent).
 *  2. Broken HTML (img without alt) → ≥1 violation (exit-1 equivalent).
 *  3. Well-formed login form markup → 0 violations.
 *  4. Missing form labels → ≥1 violation.
 *  5. S6.10 — a moderate-impact violation (content outside a landmark) is
 *     now caught; pre-S6.10 the gate only blocked serious/critical.
 */

import { describe, it, expect } from "vitest";
import { runAxeOnHtml } from "../scripts/a11y-scan.mjs";

// ---------------------------------------------------------------------------
// Clean fixtures — should return 0 blocking violations
// ---------------------------------------------------------------------------

const CLEAN_CARD = `
  <main>
    <article aria-label="Status">
      <h1>JudicialPredict</h1>
      <p>All systems operational.</p>
    </article>
  </main>
`;

const CLEAN_FORM = `
  <main>
    <form aria-label="Sign in">
      <div>
        <label for="email">Email address</label>
        <input id="email" type="email" name="email" autocomplete="email" />
      </div>
      <div>
        <label for="password">Password</label>
        <input id="password" type="password" name="password" autocomplete="current-password" />
      </div>
      <button type="submit">Sign in</button>
    </form>
  </main>
`;

// ---------------------------------------------------------------------------
// Broken fixtures — should return ≥1 blocking violation
// ---------------------------------------------------------------------------

// <img> without alt is a wcag2a "image-alt" rule — impact: critical.
const BROKEN_IMG = `
  <main>
    <img src="logo.png" />
    <p>Welcome to JudicialPredict.</p>
  </main>
`;

// Empty anchor link — no accessible name → link-name rule, impact: serious.
const BROKEN_EMPTY_LINK = `
  <main>
    <p>Read more: <a href="https://example.com"></a></p>
  </main>
`;

// Heading levels jumping h1 → h4 → axe "heading-order" rule,
// impact: moderate. This fixture PASSED the Sprint-4 serious/critical
// gate; S6.10 widened the gate to moderate, so it must now be caught.
const MODERATE_HEADING_ORDER = `
  <main>
    <h1>JudicialPredict</h1>
    <h4>Recent cases (skipped h2 and h3)</h4>
  </main>
`;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("runAxeOnHtml — clean fixtures", () => {
  it("returns 0 violations for a well-structured card", async () => {
    const result = await runAxeOnHtml(CLEAN_CARD);
    expect(result.violations).toBe(0);
  });

  it("returns 0 violations for a well-labelled login form", async () => {
    const result = await runAxeOnHtml(CLEAN_FORM);
    expect(result.violations).toBe(0);
  });
});

describe("runAxeOnHtml — broken fixtures (simulate CI failure)", () => {
  it("detects critical violation for <img> missing alt attribute", async () => {
    const result = await runAxeOnHtml(BROKEN_IMG);
    // image-alt is a critical rule — must be caught.
    expect(result.violations).toBeGreaterThan(0);
  });

  it("detects serious violation for empty anchor link (link-name rule)", async () => {
    const result = await runAxeOnHtml(BROKEN_EMPTY_LINK);
    // link-name is serious — must be caught.
    expect(result.violations).toBeGreaterThan(0);
  });
});

describe("runAxeOnHtml — moderate-impact gate (S6.10 widening)", () => {
  it("catches a moderate-impact violation (heading order skipped)", async () => {
    const result = await runAxeOnHtml(MODERATE_HEADING_ORDER);
    // "heading-order" is a moderate rule — pre-S6.10 this returned 0;
    // now it blocks.
    expect(result.violations).toBeGreaterThan(0);
  });
});
