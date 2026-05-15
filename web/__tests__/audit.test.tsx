/**
 * /audit page tests - S6.12.
 *
 * Tests the AuditTable presentational component directly (same pattern as
 * cases-list.test.tsx and case-results.test.tsx: no fetch mocking needed,
 * props injected directly).
 *
 * Covers:
 *  1. Three audit rows render with timestamp, actor, action, target.
 *  2. Reason-code badges render with the right text per row.
 *  3. Empty-state copy shows when totalCount is 0.
 *  4. axe-core a11y gate passes on loaded + empty states.
 *  5. Pagination: Next is an enabled link when nextOffset is set.
 *  6. Pagination: Next is a disabled button when nextOffset is null.
 *  7. Pagination: Previous is a disabled button on the first page.
 *  8. Pagination: Previous is an enabled link when offset > 0.
 *
 * Does NOT mock fetch or Next.js cookie APIs - data is passed as props.
 */

import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { axe, toHaveNoViolations } from "jest-axe";
import type { AuditConnection, AuditEvent } from "@/lib/queries/audit";

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

function makeEvent(overrides: Partial<AuditEvent> = {}): AuditEvent {
  return {
    id: "1",
    tenantId: "00000000-0000-0000-0000-000000000001",
    actor: "operator-a@example.test",
    action: "case.create",
    target: "cases:aaaaaaaa-0000-0000-0000-000000000001",
    reasonCode: "ok",
    ts: "2026-05-09T10:00:00Z",
    latencyMs: 42,
    ...overrides,
  };
}

const THREE_EVENTS: AuditEvent[] = [
  makeEvent({ id: "3", action: "case.create", reasonCode: "ok", ts: "2026-05-09T10:00:00Z", latencyMs: 42 }),
  makeEvent({ id: "2", action: "predict_case_outcome", reasonCode: "timeout", actor: "api-gateway", target: "outbound_call", ts: "2026-05-09T09:50:00Z", latencyMs: 5000 }),
  makeEvent({ id: "1", action: "feature_store.GetFeature", reasonCode: "err", actor: null, target: "outbound_call", ts: "2026-05-09T09:30:00Z", latencyMs: null }),
];

const LOADED_CONNECTION: AuditConnection = {
  nodes: THREE_EVENTS,
  totalCount: 80,
  nextOffset: 25,
};

const EMPTY_CONNECTION: AuditConnection = {
  nodes: [],
  totalCount: 0,
  nextOffset: null,
};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

async function renderTable(
  connection: AuditConnection,
  offset = 0,
  pageSize = 25
) {
  const { AuditTable } = await import("../app/audit/audit-table");
  return render(
    <AuditTable connection={connection} offset={offset} pageSize={pageSize} />
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("AuditTable - row rendering", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders all 3 rows with actor, action, and target columns", async () => {
    await renderTable(LOADED_CONNECTION);

    expect(screen.getByText("case.create")).toBeTruthy();
    expect(screen.getByText("predict_case_outcome")).toBeTruthy();
    expect(screen.getByText("feature_store.GetFeature")).toBeTruthy();

    expect(screen.getByText("operator-a@example.test")).toBeTruthy();
    expect(screen.getByText("api-gateway")).toBeTruthy();

    expect(
      screen.getByText("cases:aaaaaaaa-0000-0000-0000-000000000001")
    ).toBeTruthy();
    expect(screen.getAllByText("outbound_call").length).toBeGreaterThanOrEqual(1);
  });

  it("renders reason-code badges per row", async () => {
    await renderTable(LOADED_CONNECTION);
    expect(screen.getByText("ok")).toBeTruthy();
    expect(screen.getByText("timeout")).toBeTruthy();
    expect(screen.getByText("err")).toBeTruthy();
  });

  it("falls back to a dash for null actor / null latency", async () => {
    await renderTable(LOADED_CONNECTION);
    // The third row has null actor and null latencyMs - both render as a dash.
    const dashes = screen.getAllByText("-");
    expect(dashes.length).toBeGreaterThanOrEqual(2);
  });
});

describe("AuditTable - empty state", () => {
  it("shows the empty-state copy when totalCount is 0", async () => {
    await renderTable(EMPTY_CONNECTION);
    expect(screen.getByText(/no audit events yet/i)).toBeTruthy();
  });
});

describe("AuditTable - pagination", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders an enabled Next link when nextOffset is 25", async () => {
    await renderTable(LOADED_CONNECTION, 0, 25);
    const nextLink = screen.getByRole("link", { name: /next page/i });
    expect(nextLink).toBeTruthy();
    expect((nextLink as HTMLAnchorElement).getAttribute("href")).toBe(
      "/audit?offset=25"
    );
  });

  it("renders a disabled Next button when nextOffset is null", async () => {
    const lastPage: AuditConnection = {
      nodes: THREE_EVENTS,
      totalCount: 28,
      nextOffset: null,
    };
    await renderTable(lastPage, 25, 25);
    const nextBtn = screen.getByRole("button", { name: /next page/i });
    expect(nextBtn).toBeDisabled();
  });

  it("renders a disabled Previous button on the first page (offset=0)", async () => {
    await renderTable(LOADED_CONNECTION, 0, 25);
    const prevBtn = screen.getByRole("button", { name: /previous page/i });
    expect(prevBtn).toBeDisabled();
  });

  it("renders an enabled Previous link when offset > 0", async () => {
    await renderTable(LOADED_CONNECTION, 25, 25);
    const prevLink = screen.getByRole("link", { name: /previous page/i });
    expect(prevLink).toBeTruthy();
    expect((prevLink as HTMLAnchorElement).getAttribute("href")).toBe(
      "/audit?offset=0"
    );
  });
});

describe("AuditTable - a11y", () => {
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
