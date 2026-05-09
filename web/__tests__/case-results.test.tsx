/**
 * /case/[id] results view tests — S3.3 / JP-44.
 *
 * Covers:
 *  1. Happy path: renders P(win), CI, bullets, recommendation badge from sessionStorage.
 *  2. Empty sessionStorage → shows empty-state CTA back to /case/new.
 *  3. Malformed JSON in sessionStorage → shows empty-state, no thrown error.
 *  4. axe-core a11y gate for the empty state.
 *  5. axe-core a11y gate for the full results layout.
 *
 * Does NOT call api-gateway or any network endpoint.
 * Follows the vi.hoisted + clearAllMocks pattern from __tests__/auth.test.tsx.
 */

import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { axe, toHaveNoViolations } from "jest-axe";

expect.extend(toHaveNoViolations);

// ---------------------------------------------------------------------------
// Mocks — hoisted so they apply before any module import resolution
// ---------------------------------------------------------------------------

const { mockRouterPush } = vi.hoisted(() => ({
  mockRouterPush: vi.fn(),
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: mockRouterPush }),
}));

vi.mock("next/link", () => ({
  default: ({
    href,
    children,
    className,
  }: {
    href: string;
    children: React.ReactNode;
    className?: string;
  }) => (
    <a href={href} className={className}>
      {children}
    </a>
  ),
}));

// ---------------------------------------------------------------------------
// Fixture: a valid prediction stashed by the S3.2 intake form.
//
// pWin=0.42, ciLower=0.31, ciUpper=0.53, expectedDamages=250,000, cost=50,000 (demo default)
//   EV_try    = 0.42 × 250,000 − 50,000 = 55,000
//   EV_settle = 250,000 × 0.40           = 100,000   (> EV_try)
//   ciLower   = 0.31 < 0.40 ✓ → Settle
// ---------------------------------------------------------------------------

const VALID_RESULT = {
  pWin: 0.42,
  ciLower: 0.31,
  ciUpper: 0.53,
  coverage: 0.90,
  modelVersion: "test-run-abc123",
  predictedAtUnix: 1_746_748_800,
  expectedDamages: 250_000,
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("ResultsView", () => {
  beforeEach(() => {
    sessionStorage.clear();
    // clearAllMocks clears call history only; resetAllMocks would wipe mock implementations.
    vi.clearAllMocks();
  });

  it("renders P(win), CI, bullets, and recommendation badge from sessionStorage", async () => {
    const caseId = "00000000-0000-0000-0000-000000000042";
    sessionStorage.setItem(`case:${caseId}`, JSON.stringify(VALID_RESULT));

    const { ResultsView } = await import("../app/case/[id]/results-view");
    render(<ResultsView caseId={caseId} />);

    // Wait for the useEffect to read sessionStorage and update state.
    await waitFor(() => {
      // P(win) shown as "42%" (Math.round(0.42 × 100) = 42)
      expect(screen.getByText("42%")).toBeTruthy();
    });

    // 90% CI range — appears in both the header strip and the first bullet,
    // so use getAllByText and assert at least one match.
    expect(screen.getAllByText(/0\.31.*0\.53/).length).toBeGreaterThan(0);
    // Model version
    expect(screen.getByText(/test-run-abc123/)).toBeTruthy();
    // Recommendation badge — Settle (EV_settle dominates and ciLower < 0.40)
    expect(screen.getByText("Settle")).toBeTruthy();
    // Reasoning bullet[0] contains "P(win)" — appears in the headline metric and
    // at least one bullet, so use getAllByText.
    expect(screen.getAllByText(/P\(win\)/).length).toBeGreaterThan(0);
  });

  it("shows empty-state CTA when sessionStorage has no entry for the case", async () => {
    const { ResultsView } = await import("../app/case/[id]/results-view");
    render(<ResultsView caseId="no-such-case-id" />);

    await waitFor(() => {
      expect(screen.getByText(/Results not available/i)).toBeTruthy();
    });

    // CTA link must point to /case/new
    const cta = screen.getByRole("link", { name: /submit a new case/i });
    expect(cta).toBeTruthy();
    expect((cta as HTMLAnchorElement).getAttribute("href")).toBe("/case/new");
  });

  it("shows empty state for malformed JSON and does not throw", async () => {
    const caseId = "malformed-case";
    sessionStorage.setItem(`case:${caseId}`, "{{not-valid-json}}");

    const { ResultsView } = await import("../app/case/[id]/results-view");
    // Must not throw even with corrupt data.
    expect(() => render(<ResultsView caseId={caseId} />)).not.toThrow();

    await waitFor(() => {
      expect(screen.getByText(/Results not available/i)).toBeTruthy();
    });
  });

  it("passes axe-core a11y check for the empty state", async () => {
    const { ResultsView } = await import("../app/case/[id]/results-view");
    const { container } = render(<ResultsView caseId="axe-no-data" />);

    await waitFor(() => {
      expect(screen.getByText(/Results not available/i)).toBeTruthy();
    });

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it("passes axe-core a11y check for the full results layout", async () => {
    const caseId = "axe-full-results";
    sessionStorage.setItem(`case:${caseId}`, JSON.stringify(VALID_RESULT));

    const { ResultsView } = await import("../app/case/[id]/results-view");
    const { container } = render(<ResultsView caseId={caseId} />);

    await waitFor(() => {
      expect(screen.getByText("42%")).toBeTruthy();
    });

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});
