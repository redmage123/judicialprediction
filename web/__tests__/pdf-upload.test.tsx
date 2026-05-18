/**
 * S6.13 — PDF upload smoke tests.
 *
 * Mocks @/lib/pdf-extract at the module boundary so the tests don't have
 * to spin up pdfjs-dist's worker in jsdom.  Covers the three outcomes the
 * intake-form handler must surface to the operator:
 *   1. text-extractable PDF -> opinion textarea is populated, status line
 *      shows the page count, no error.
 *   2. scanned PDF (no text layer) -> error message tells the operator to
 *      paste text manually; textarea is left empty.
 *   3. PdfExtractError ("too_large" etc.) -> the error message bubbles to
 *      the role="alert" line.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MockedProvider } from "@apollo/client/testing/react";
import { IntakeForm } from "@/app/case/new/intake-form";

// ---------------------------------------------------------------------------
// Module mocks
// ---------------------------------------------------------------------------

const extractMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/pdf-extract", async () => {
  // Re-export the real PdfExtractError class so the handler can `instanceof`
  // it; keep extractTextFromPdf mocked.
  const actual =
    await vi.importActual<typeof import("@/lib/pdf-extract")>(
      "@/lib/pdf-extract"
    );
  return {
    ...actual,
    extractTextFromPdf: extractMock,
  };
});

// next/navigation is also mocked everywhere else in the suite; mirror that
// minimal stub here so IntakeForm's useRouter() doesn't crash.
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), replace: vi.fn(), refresh: vi.fn() }),
  useSearchParams: () => new URLSearchParams(),
  usePathname: () => "/case/new",
}));

// Apollo Client v4 removed `addTypename` from `MockedProvider`; the IntakeForm
// in these tests issues no queries, so the empty mock list is all we need.
function renderForm() {
  return render(
    <MockedProvider mocks={[]}>
      <IntakeForm />
    </MockedProvider>
  );
}

function fakePdf(name = "opinion.pdf", sizeBytes = 1234) {
  // Minimal File polyfill — jsdom's File reads .name/.size/.type fine.
  return new File([new Uint8Array(sizeBytes)], name, {
    type: "application/pdf",
  });
}

beforeEach(() => {
  extractMock.mockReset();
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("S6.13 — PDF upload on /case/new", () => {
  it("populates the opinion textarea when the PDF is text-extractable", async () => {
    extractMock.mockResolvedValueOnce({
      kind: "text",
      text: "Petitioner v. Commissioner, 2024. The court holds…",
      pageCount: 4,
    });

    renderForm();
    // Open the <details> panel so the upload control is rendered.
    fireEvent.click(
      screen.getByText(/Prefill from a prior opinion/i)
    );

    const input = screen.getByLabelText(/Upload PDF opinion/i);
    fireEvent.change(input, { target: { files: [fakePdf()] } });

    await waitFor(() =>
      expect(extractMock).toHaveBeenCalledTimes(1)
    );
    const textarea = await screen.findByLabelText(
      /Opinion text for feature extraction/i
    );
    expect((textarea as HTMLTextAreaElement).value).toMatch(
      /Petitioner v\. Commissioner/
    );
    expect(screen.getByText(/Loaded 4 pages from opinion\.pdf/)).toBeTruthy();
  });

  it("warns when the PDF appears scanned (no text layer)", async () => {
    extractMock.mockResolvedValueOnce({
      kind: "scanned-pdf",
      pageCount: 2,
    });

    renderForm();
    fireEvent.click(
      screen.getByText(/Prefill from a prior opinion/i)
    );
    const input = screen.getByLabelText(/Upload PDF opinion/i);
    fireEvent.change(input, { target: { files: [fakePdf("scan.pdf")] } });

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toMatch(/scanned/i);
    expect(alert.textContent).toMatch(/paste the text manually/i);

    const textarea = screen.getByLabelText(
      /Opinion text for feature extraction/i
    ) as HTMLTextAreaElement;
    expect(textarea.value).toBe("");
  });

  it("surfaces a PdfExtractError as an alert", async () => {
    const { PdfExtractError } =
      await vi.importActual<typeof import("@/lib/pdf-extract")>(
        "@/lib/pdf-extract"
      );
    extractMock.mockRejectedValueOnce(
      new PdfExtractError(
        "too_large",
        "PDF is 11.5 MB; the limit is 10 MB."
      )
    );

    renderForm();
    fireEvent.click(
      screen.getByText(/Prefill from a prior opinion/i)
    );
    const input = screen.getByLabelText(/Upload PDF opinion/i);
    fireEvent.change(input, { target: { files: [fakePdf("huge.pdf")] } });

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toMatch(/11\.5 MB/);
    expect(alert.textContent).toMatch(/limit is 10 MB/);
  });
});
