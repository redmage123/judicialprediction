/**
 * /case/[id] results view tests — S3.3 / updated S4.4 (JP-58).
 *
 * Covers:
 *  1. Happy path: renders P(win), CI, bullets, recommendation badge from prop.
 *  2. null prop → shows empty-state CTA back to /case/new.
 *  3. axe-core a11y gate for the empty state.
 *  4. axe-core a11y gate for the full results layout.
 *
 * S4.4 changes:
 *  - ResultsView now takes caseResult: CaseResult | null (no more caseId string).
 *  - No sessionStorage seeding or reading.
 *  - Server-computed recommendation.rationaleBullets replaces client-side rec.bullets.
 *  - No Apollo MockedProvider needed — component is purely presentational.
 *
 * Does NOT call api-gateway or any network endpoint.
 * Follows the vi.hoisted + clearAllMocks pattern from __tests__/auth.test.tsx.
 */

import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { axe, toHaveNoViolations } from "jest-axe";

expect.extend(toHaveNoViolations);

// ---------------------------------------------------------------------------
// Mocks
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
// Fixture
//
// pWin=0.42, ciLower=0.31, ciUpper=0.53 → Settle.
// Server recommendation uses the same decision-arith logic but returns
// pre-computed strings, so we just supply representative values.
// ---------------------------------------------------------------------------

const VALID_CASE: import("@/lib/queries/predict").CaseResult = {
  id: "00000000-0000-0000-0000-000000000042",
  tenantId: "00000000-0000-0000-0000-000000000001",
  inputFeatures: {
    judgeSeverity: 0.42,
    attorneyWinRate: 0.5,
    ideologyDistance: 0.3,
    materialityScore: 0.8,
    proceduralMotionCount: 3,
    caseType: "civil",
    jurisdiction: "us-federal",
  },
  prediction: {
    pWin: 0.42,
    ciLower: 0.31,
    ciUpper: 0.53,
    coverage: 0.90,
    modelVersion: "test-run-abc123",
    predictedAtUnix: 1_746_748_800,
  },
  recommendation: {
    kind: "Settle",
    rationaleBullets: [
      "P(win) 0.42 with 90% CI [0.31, 0.53]",
      "Expected value at trial $55000.00 vs. expected settlement value $100000.00",
      "Settlement preferred: CI lower bound (0.31) is below the loss-exposure threshold of 0.40",
    ],
    expectedValueTry: "55000.00",
    expectedValueSettle: "100000.00",
  },
  createdBy: null,
  createdAt: "2026-05-10T12:00:00Z",
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("ResultsView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders P(win), CI, bullets, and recommendation badge from prop", async () => {
    const { ResultsView } = await import("../app/case/[id]/results-view");
    render(<ResultsView caseResult={VALID_CASE} />);

    // P(win) shown as "42%" (Math.round(0.42 × 100) = 42)
    expect(screen.getByText("42%")).toBeTruthy();

    // 90% CI range in the header card
    expect(screen.getAllByText(/0\.31.*0\.53/).length).toBeGreaterThan(0);
    // Model version
    expect(screen.getByText(/test-run-abc123/)).toBeTruthy();
    // Recommendation badge
    expect(screen.getByText("Settle")).toBeTruthy();
    // Reasoning bullet[0] via rationaleBullets
    expect(screen.getByText(/P\(win\) 0\.42 with 90% CI/)).toBeTruthy();
  });

  it("shows empty-state CTA when caseResult is null", async () => {
    const { ResultsView } = await import("../app/case/[id]/results-view");
    render(<ResultsView caseResult={null} />);

    expect(screen.getByText(/Results not available/i)).toBeTruthy();

    // CTA link must point to /case/new
    const cta = screen.getByRole("link", { name: /submit a new case/i });
    expect(cta).toBeTruthy();
    expect((cta as HTMLAnchorElement).getAttribute("href")).toBe("/case/new");
  });

  it("passes axe-core a11y check for the empty state", async () => {
    const { ResultsView } = await import("../app/case/[id]/results-view");
    const { container } = render(<ResultsView caseResult={null} />);

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it("passes axe-core a11y check for the full results layout", async () => {
    const { ResultsView } = await import("../app/case/[id]/results-view");
    const { container } = render(<ResultsView caseResult={VALID_CASE} />);

    // Wait for render — no async needed since no useEffect
    expect(screen.getByText("42%")).toBeTruthy();

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});
