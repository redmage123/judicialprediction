/**
 * S6.14 — /cases/import smoke tests.
 *
 * Exercises ImportForm with the three failure modes operators will
 * most often hit (bad header, validation error, row-cap), plus the
 * happy path with a mocked importCases mutation.
 */

import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MockedProvider } from "@apollo/client/testing/react";
import { ImportForm } from "@/app/cases/import/import-form";
import {
  IMPORT_CASES,
  MAX_IMPORT_ROWS,
} from "@/lib/queries/import";

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), replace: vi.fn(), refresh: vi.fn() }),
  useSearchParams: () => new URLSearchParams(),
  usePathname: () => "/cases/import",
}));

const GOOD_HEADER =
  "judge_severity,attorney_win_rate,ideology_distance,materiality_score,procedural_motion_count,case_type,jurisdiction,opinion_text";

function csv(...rows: string[]) {
  return [GOOD_HEADER, ...rows].join("\n");
}

function dataRow(overrides: Partial<Record<string, string>> = {}) {
  const base: Record<string, string> = {
    judge_severity: "0.5",
    attorney_win_rate: "0.6",
    ideology_distance: "0.3",
    materiality_score: "0.8",
    procedural_motion_count: "2",
    case_type: "civil",
    jurisdiction: "us-federal",
    opinion_text: "",
  };
  const merged = { ...base, ...overrides };
  return [
    merged.judge_severity,
    merged.attorney_win_rate,
    merged.ideology_distance,
    merged.materiality_score,
    merged.procedural_motion_count,
    merged.case_type,
    merged.jurisdiction,
    merged.opinion_text,
  ].join(",");
}

function csvFile(name: string, body: string) {
  return new File([body], name, { type: "text/csv" });
}

function renderWith(mocks: React.ComponentProps<typeof MockedProvider>["mocks"] = []) {
  return render(
    <MockedProvider mocks={mocks} addTypename={false}>
      <ImportForm />
    </MockedProvider>
  );
}

async function uploadCsv(body: string, fileName = "cases.csv") {
  const input = screen.getByLabelText(/Upload cases CSV/i);
  Object.defineProperty(input, "files", {
    value: [csvFile(fileName, body)],
    configurable: true,
  });
  fireEvent.change(input);
}

describe("S6.14 — CSV bulk import form", () => {
  it("flags a missing required column", async () => {
    renderWith();
    const bad = "judge_severity,attorney_win_rate,jurisdiction\n0.5,0.5,us-federal";
    await uploadCsv(bad);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toMatch(/missing required column/i);
    expect(alert.textContent).toMatch(/ideology_distance/);
    expect(alert.textContent).toMatch(/case_type/);
  });

  it("flags per-row validation errors", async () => {
    renderWith();
    const body = csv(
      dataRow({ judge_severity: "1.5" }),     // out of range
      dataRow({ case_type: "not_a_kind" }),   // bad enum
    );
    await uploadCsv(body);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toMatch(/2 validation errors/);
    expect(alert.textContent).toMatch(/judge_severity 1\.5 out of range/);
    expect(alert.textContent).toMatch(/case_type "not_a_kind"/);
  });

  it("rejects CSVs above the row cap", async () => {
    renderWith();
    const rows = Array.from({ length: MAX_IMPORT_ROWS + 1 }, () => dataRow());
    await uploadCsv(csv(...rows));
    const alerts = await screen.findAllByRole("alert");
    expect(alerts.some((el) => /exceeds the 50-row limit/.test(el.textContent ?? ""))).toBe(true);
  });

  it("submits a valid CSV and renders per-row results", async () => {
    const mocks = [
      {
        request: {
          query: IMPORT_CASES,
          variables: {
            rows: [
              {
                judgeSeverity: 0.5,
                attorneyWinRate: 0.6,
                ideologyDistance: 0.3,
                materialityScore: 0.8,
                proceduralMotionCount: 2,
                caseType: "civil",
                jurisdiction: "us-federal",
              },
              {
                judgeSeverity: 0.7,
                attorneyWinRate: 0.5,
                ideologyDistance: 0.4,
                materialityScore: 0.6,
                proceduralMotionCount: 1,
                caseType: "criminal",
                jurisdiction: "us-federal",
              },
            ],
          },
        },
        result: {
          data: {
            importCases: {
              total: 2,
              succeeded: 1,
              failed: 1,
              results: [
                {
                  rowIndex: 0,
                  ok: true,
                  caseId: "550e8400-e29b-41d4-a716-446655440000",
                  error: null,
                },
                {
                  rowIndex: 1,
                  ok: false,
                  caseId: null,
                  error: "ml inference timed out",
                },
              ],
            },
          },
        },
      },
    ];
    renderWith(mocks);
    const body = csv(dataRow(), dataRow({ judge_severity: "0.7", case_type: "criminal", procedural_motion_count: "1", materiality_score: "0.6", attorney_win_rate: "0.5", ideology_distance: "0.4" }));
    await uploadCsv(body);

    const submitBtn = await screen.findByRole("button", { name: /Import 2 cases/i });
    fireEvent.click(submitBtn);

    await waitFor(() =>
      expect(screen.getByText(/Import complete/i)).toBeTruthy()
    );
    expect(screen.getByText(/1 succeeded, 1 failed/)).toBeTruthy();
    expect(screen.getByRole("link", { name: /View case/i })).toBeTruthy();
    expect(screen.getByText(/ml inference timed out/)).toBeTruthy();
  });
});
