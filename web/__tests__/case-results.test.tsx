/**
 * /case/[id] results view tests — S3.3 / updated S4.4 (JP-58) / S4.7 (JP-61).
 *
 * Covers:
 *  1. Happy path: renders P(win), CI, bullets, recommendation badge from prop.
 *  2. null prop → shows empty-state CTA back to /case/new.
 *  3. axe-core a11y gate for the empty state.
 *  4. axe-core a11y gate for the full results layout.
 *  5. (S4.7) Re-run button is visible when caseResult is provided.
 *  6. (S4.7) Re-run button calls the REPREDICT_CASE mutation when clicked.
 *  7. (S4.7) History disclosure starts collapsed; clicking it shows the section.
 *
 * S4.4 changes:
 *  - ResultsView now takes caseResult: CaseResult | null (no more caseId string).
 *  - No sessionStorage seeding or reading.
 *  - Server-computed recommendation.rationaleBullets replaces client-side rec.bullets.
 *
 * S4.7 changes:
 *  - ResultsView is now a "use client" component hosting RepredictButton and
 *    PredictionHistoryDisclosure islands that use useMutation / useQuery.
 *  - @apollo/client hooks are mocked via vi.hoisted so tests don't need an
 *    ApolloProvider context.
 *
 * Does NOT call api-gateway or any network endpoint.
 * Follows the vi.hoisted + clearAllMocks pattern from __tests__/auth.test.tsx.
 */

import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { axe, toHaveNoViolations } from "jest-axe";

expect.extend(toHaveNoViolations);

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

const { mockRouterPush, mockRouterRefresh, mockMutate, mockUseMutation, mockUseQuery } =
  vi.hoisted(() => {
    const mockMutate = vi.fn().mockResolvedValue({ data: {} });
    return {
      mockRouterPush: vi.fn(),
      mockRouterRefresh: vi.fn(),
      mockMutate,
      mockUseMutation: vi.fn(() => [mockMutate, { loading: false, data: null }]),
      mockUseQuery: vi.fn(() => ({ data: null, loading: false, error: undefined })),
    };
  });

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: mockRouterPush, refresh: mockRouterRefresh }),
}));

// Mock @apollo/client hooks so the component renders without an ApolloProvider.
// gql is imported from the actual library so DocumentNode constants in predict.ts
// are constructed correctly (they're parsed at module load time).
vi.mock("@apollo/client", async (importActual) => {
  const actual = await importActual<typeof import("@apollo/client")>();
  return {
    ...actual,
    useMutation: mockUseMutation,
    useQuery: mockUseQuery,
  };
});

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

  // ── S4.7 tests ────────────────────────────────────────────────────────────

  it("(S4.7) shows the Re-run button when caseResult is provided", async () => {
    const { ResultsView } = await import("../app/case/[id]/results-view");
    render(<ResultsView caseResult={VALID_CASE} />);

    // The RepredictButton renders with aria-label "Re-run with latest model"
    const btn = screen.getByRole("button", { name: /re-run with latest model/i });
    expect(btn).toBeTruthy();
    // The button must not be disabled in the default (non-loading) state.
    expect((btn as HTMLButtonElement).disabled).toBe(false);
  });

  it("(S4.7) clicking Re-run calls the REPREDICT_CASE mutation", async () => {
    const { ResultsView } = await import("../app/case/[id]/results-view");
    render(<ResultsView caseResult={VALID_CASE} />);

    const btn = screen.getByRole("button", { name: /re-run with latest model/i });
    fireEvent.click(btn);

    // useMutation returns [mockMutate, {loading}]; clicking the button should
    // invoke the mutate function exactly once.
    expect(mockMutate).toHaveBeenCalledTimes(1);
  });

  it("(S4.7) prediction history disclosure starts collapsed; clicking expands it", async () => {
    const { ResultsView } = await import("../app/case/[id]/results-view");
    render(<ResultsView caseResult={VALID_CASE} />);

    // The toggle button renders with aria-expanded="false" by default.
    const toggle = screen.getByRole("button", { name: /show prediction history/i });
    expect(toggle).toBeTruthy();
    expect((toggle as HTMLButtonElement).getAttribute("aria-expanded")).toBe("false");

    // No history region rendered yet.
    expect(screen.queryByRole("list", { name: /past prediction runs/i })).toBeNull();

    // Click to expand.
    fireEvent.click(toggle);

    // After click, aria-expanded flips and the region appears.
    expect((toggle as HTMLButtonElement).getAttribute("aria-expanded")).toBe("true");
    expect(
      screen.getByRole("list", { name: /past prediction runs/i })
    ).toBeTruthy();
  });
});
