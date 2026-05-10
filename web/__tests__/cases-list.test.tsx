/**
 * /cases page tests — S4.5 (JP-59)
 *
 * Tests the CasesTable presentational component directly (same pattern as
 * case-results.test.tsx: no fetch mocking needed, props injected directly).
 *
 * Covers:
 *  1. Three rows render with correct case types and recommendation badges.
 *  2. Empty-state CTA appears when totalCount=0.
 *  3. "Next" link is enabled when nextOffset=20.
 *  4. "Next" button is disabled when nextOffset=null.
 *  5. "Previous" button is disabled on the first page (offset=0).
 *  6. axe-core a11y gate on the loaded state.
 *  7. axe-core a11y gate on the empty state.
 *
 * Does NOT mock fetch or Next.js cookie APIs — data is passed as props.
 */

import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { axe, toHaveNoViolations } from "jest-axe";
import type { CaseConnection, CaseSummary } from "@/lib/queries/predict";

expect.extend(toHaveNoViolations);

// ---------------------------------------------------------------------------
// next/link mock (same pattern as other test files)
// ---------------------------------------------------------------------------

vi.mock("next/link", () => ({
  default: ({
    href,
    children,
    className,
    "aria-label": ariaLabel,
  }: {
    href: string;
    children: React.ReactNode;
    className?: string;
    "aria-label"?: string;
  }) => (
    <a href={href} className={className} aria-label={ariaLabel}>
      {children}
    </a>
  ),
}));

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function makeSummary(overrides: Partial<CaseSummary> = {}): CaseSummary {
  return {
    id: crypto.randomUUID(),
    inputFeatures: { caseType: "civil", jurisdiction: "us-federal" },
    prediction: { pWin: 0.72 },
    recommendation: { kind: "Try" },
    createdAt: "2026-05-09T10:00:00Z",
    createdBy: null,
    ...overrides,
  };
}

const THREE_CASES: CaseSummary[] = [
  makeSummary({
    id: "aaaaaaaa-0000-0000-0000-000000000001",
    inputFeatures: { caseType: "civil", jurisdiction: "us-federal" },
    prediction: { pWin: 0.72 },
    recommendation: { kind: "Try" },
    createdAt: "2026-05-09T10:00:00Z",
  }),
  makeSummary({
    id: "aaaaaaaa-0000-0000-0000-000000000002",
    inputFeatures: { caseType: "criminal", jurisdiction: "ca-state" },
    prediction: { pWin: 0.38 },
    recommendation: { kind: "Settle" },
    createdAt: "2026-05-08T09:30:00Z",
  }),
  makeSummary({
    id: "aaaaaaaa-0000-0000-0000-000000000003",
    inputFeatures: { caseType: "bankruptcy", jurisdiction: "nj-state" },
    prediction: { pWin: 0.5 },
    recommendation: { kind: "Borderline" },
    createdAt: "2026-05-07T14:00:00Z",
  }),
];

const LOADED_CONNECTION: CaseConnection = {
  nodes: THREE_CASES,
  totalCount: 47,
  nextOffset: 20,
};

const EMPTY_CONNECTION: CaseConnection = {
  nodes: [],
  totalCount: 0,
  nextOffset: null,
};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

async function renderTable(
  connection: CaseConnection,
  offset = 0,
  pageSize = 20
) {
  const { CasesTable } = await import("../app/cases/cases-table");
  return render(
    <CasesTable connection={connection} offset={offset} pageSize={pageSize} />
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("CasesTable — row rendering", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders all 3 rows with correct case types and recommendation badges", async () => {
    await renderTable(LOADED_CONNECTION);

    // Case types (capitalize class applied in CSS, but text in DOM is lowercase)
    expect(screen.getByText("civil")).toBeTruthy();
    expect(screen.getByText("criminal")).toBeTruthy();
    expect(screen.getByText("bankruptcy")).toBeTruthy();

    // Recommendation badges
    expect(screen.getByText("Try")).toBeTruthy();
    expect(screen.getByText("Settle")).toBeTruthy();
    expect(screen.getByText("Borderline")).toBeTruthy();

    // P(win) rounded to whole percent
    expect(screen.getByText("72%")).toBeTruthy();
    expect(screen.getByText("38%")).toBeTruthy();
    expect(screen.getByText("50%")).toBeTruthy();

    // Three "View" links
    const viewLinks = screen.getAllByRole("link", { name: "View" });
    expect(viewLinks).toHaveLength(3);
    expect((viewLinks[0] as HTMLAnchorElement).href).toContain(
      "aaaaaaaa-0000-0000-0000-000000000001"
    );
  });
});

describe("CasesTable — empty state", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows empty-state CTA when totalCount is 0", async () => {
    await renderTable(EMPTY_CONNECTION);

    expect(screen.getByText(/no cases yet/i)).toBeTruthy();
    const cta = screen.getByRole("link", { name: /submit your first case/i });
    expect(cta).toBeTruthy();
    expect((cta as HTMLAnchorElement).getAttribute("href")).toBe("/case/new");
  });
});

describe("CasesTable — pagination", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders an enabled Next link when nextOffset is 20", async () => {
    await renderTable(LOADED_CONNECTION, 0);

    const nextLink = screen.getByRole("link", { name: /next page/i });
    expect(nextLink).toBeTruthy();
    expect((nextLink as HTMLAnchorElement).getAttribute("href")).toBe(
      "/cases?offset=20"
    );
  });

  it("renders a disabled Next button when nextOffset is null", async () => {
    const lastPage: CaseConnection = {
      nodes: THREE_CASES,
      totalCount: 23,
      nextOffset: null,
    };
    await renderTable(lastPage, 20);

    const nextBtn = screen.getByRole("button", { name: /next page/i });
    expect(nextBtn).toBeDisabled();
  });

  it("renders a disabled Previous button on the first page (offset=0)", async () => {
    await renderTable(LOADED_CONNECTION, 0);

    const prevBtn = screen.getByRole("button", { name: /previous page/i });
    expect(prevBtn).toBeDisabled();
  });

  it("renders an enabled Previous link when offset > 0", async () => {
    await renderTable(LOADED_CONNECTION, 20);

    const prevLink = screen.getByRole("link", { name: /previous page/i });
    expect(prevLink).toBeTruthy();
    expect((prevLink as HTMLAnchorElement).getAttribute("href")).toBe(
      "/cases?offset=0"
    );
  });
});

describe("CasesTable — a11y", () => {
  it("passes axe-core with no violations (loaded state)", async () => {
    const { container } = await renderTable(LOADED_CONNECTION);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it("passes axe-core with no violations (empty state)", async () => {
    const { container } = await renderTable(EMPTY_CONNECTION);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});
