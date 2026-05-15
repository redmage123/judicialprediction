"use client";

/**
 * S6.13 — Extract text from a user-uploaded PDF entirely in the browser.
 *
 * Runs pdfjs-dist client-side so the PDF never leaves the user's machine,
 * which matters for legal filings.  Text-extractable PDFs come back as
 * `{ kind: "text" }`; scanned / image-only PDFs (combined text below
 * MIN_TEXT_CHARS after enumerating every page) come back as
 * `{ kind: "scanned-pdf" }` and the UI surfaces a "needs OCR" hint.
 *
 * S6.16 follow-up: add Tesseract.js OCR fallback for the scanned-pdf
 * branch.  Deliberately deferred to keep S6.13's surface small and the
 * client bundle light (tesseract WASM is ~30 MB on first use).
 */

/** PDFs above this size are rejected client-side; matches the operator
 *  guidance in `docs/runbooks/intake.md` (TODO: write that doc in S6.16). */
export const MAX_PDF_BYTES = 10 * 1024 * 1024;

/** Below this many extracted chars we treat the PDF as scanned / OCR-needing
 *  rather than truly empty — covers cover-page-only and image-only filings. */
export const MIN_TEXT_CHARS = 200;

/** Hosted pdf.worker matches the bundled pdfjs-dist version exactly.  Pinned
 *  on cdnjs (mirrors npm releases); switch to a same-origin asset if cdnjs
 *  ever becomes a privacy concern. */
const PDFJS_WORKER_CDN =
  "https://cdnjs.cloudflare.com/ajax/libs/pdf.js/5.7.284/pdf.worker.min.mjs";

export type PdfExtractResult =
  | { kind: "text"; text: string; pageCount: number }
  | { kind: "scanned-pdf"; pageCount: number };

/** Closed error-code union — discriminator on `code`, not a freeform string. */
export type PdfExtractErrorCode =
  | "too_large"
  | "invalid_pdf"
  | "parse_failed";

export class PdfExtractError extends Error {
  readonly code: PdfExtractErrorCode;
  constructor(code: PdfExtractErrorCode, message: string) {
    super(message);
    this.code = code;
    this.name = "PdfExtractError";
  }
}

/**
 * Read a PDF File and return either extracted text or a "scanned" verdict.
 * Throws `PdfExtractError` for size / format / parse failures.
 */
export async function extractTextFromPdf(
  file: File
): Promise<PdfExtractResult> {
  if (file.size > MAX_PDF_BYTES) {
    throw new PdfExtractError(
      "too_large",
      `PDF is ${(file.size / 1024 / 1024).toFixed(1)} MB; the limit is 10 MB.`
    );
  }
  if (file.type && file.type !== "application/pdf") {
    throw new PdfExtractError(
      "invalid_pdf",
      `Expected application/pdf, got ${file.type}.`
    );
  }

  let buf: ArrayBuffer;
  try {
    buf = await file.arrayBuffer();
  } catch (e) {
    throw new PdfExtractError(
      "parse_failed",
      `Could not read file: ${e instanceof Error ? e.message : String(e)}`
    );
  }

  // Dynamic import keeps pdfjs-dist out of the initial bundle — only loaded
  // when an operator actually uses the upload button.
  const pdfjs = await import("pdfjs-dist");
  pdfjs.GlobalWorkerOptions.workerSrc = PDFJS_WORKER_CDN;

  let pdf;
  try {
    pdf = await pdfjs.getDocument({ data: buf }).promise;
  } catch (e) {
    throw new PdfExtractError(
      "invalid_pdf",
      `Not a valid PDF: ${e instanceof Error ? e.message : String(e)}`
    );
  }

  const pageCount = pdf.numPages;
  let combined = "";
  for (let i = 1; i <= pageCount; i++) {
    const page = await pdf.getPage(i);
    const content = await page.getTextContent();
    const pageText = content.items
      .map((it) => ("str" in it ? it.str : ""))
      .join(" ")
      .trim();
    if (pageText) {
      combined += pageText + "\n\n";
    }
  }

  combined = combined.trim();
  if (combined.length >= MIN_TEXT_CHARS) {
    return { kind: "text", text: combined, pageCount };
  }
  return { kind: "scanned-pdf", pageCount };
}
