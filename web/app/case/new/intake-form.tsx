"use client";

// Sprint-3 follow-up: replaced sessionStorage + client UUID with createCase mutation
// that persists the case server-side and returns a real server UUID (S4.4 / JP-58).

import { useRef, useState, type FormEvent, type ChangeEvent } from "react";
import { useRouter } from "next/navigation";
import { useApolloClient, useMutation } from "@apollo/client/react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import {
  CREATE_CASE,
  EXTRACT_FEATURES,
  type PredictInput,
  type CreateCaseData,
  type CreateCaseVars,
  type ExtractFeaturesData,
  type ExtractFeaturesVars,
} from "@/lib/queries/predict";
import { CASE_TYPES, type CaseType } from "@/lib/case-types";
import { JURISDICTIONS, type Jurisdiction } from "@/lib/jurisdictions";
import {
  extractTextFromPdf,
  PdfExtractError,
  MAX_PDF_BYTES,
} from "@/lib/pdf-extract";

// ---------------------------------------------------------------------------
// Sample data for developer smoke-testing (never rendered in production).
// ---------------------------------------------------------------------------
const SAMPLE_DATA = {
  judgeSeverity: "0.65",
  attorneyWinRate: "0.72",
  ideologyDistance: "0.41",
  materialityScore: "0.88",
  proceduralMotionCount: "3",
  caseType: "civil" as CaseType,
  jurisdiction: "us-federal" as Jurisdiction,
};

// ---------------------------------------------------------------------------
// Shared select styling mirrors the shadcn/ui Input primitive.
// ---------------------------------------------------------------------------
const selectCn = cn(
  "border-input flex h-9 w-full rounded-md border bg-transparent px-3 py-1",
  "text-sm shadow-sm transition-colors outline-none",
  "focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]",
  "disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50"
);

// ---------------------------------------------------------------------------
// IntakeForm
// ---------------------------------------------------------------------------

interface FormState {
  judgeSeverity: string;
  attorneyWinRate: string;
  ideologyDistance: string;
  materialityScore: string;
  proceduralMotionCount: string;
  caseType: CaseType;
  jurisdiction: Jurisdiction;
}

const INITIAL: FormState = {
  judgeSeverity: "",
  attorneyWinRate: "",
  ideologyDistance: "",
  materialityScore: "",
  proceduralMotionCount: "",
  caseType: "civil",
  jurisdiction: "us-federal",
};

/**
 * Map an Apollo/network error into a one-sentence human message.
 *
 * The gateway includes `extensions.code` and `extensions.detail` on every
 * resolver error (S6.16) so we can distinguish "model not trained" from
 * "timeout" from "bad request" rather than telling the operator to "try
 * again" for failures that retrying won't fix.
 */
function humaniseError(err: unknown): string {
  if (!err || typeof err !== "object") {
    return "Prediction failed. Please try again.";
  }
  const anyErr = err as {
    message?: string;
    graphQLErrors?: Array<{
      message?: string;
      extensions?: { code?: string; detail?: string };
    }>;
    networkError?: { message?: string };
  };
  const gql = anyErr.graphQLErrors?.[0];
  const code = gql?.extensions?.code;
  const detail = gql?.extensions?.detail;
  switch (code) {
    case "MlInferenceUnavailable":
      return detail
        ? `Inference service is not ready: ${detail}. Ask an administrator to check the ML service.`
        : "Inference service is not ready. Ask an administrator to check the ML service.";
    case "MlInferenceTimeout":
      return "Inference timed out. Please try again in a few seconds.";
    case "MlInferenceBadRequest":
      return detail ? `Server rejected the input: ${detail}` : "Server rejected the input.";
    case "MlInferenceInternal":
      return detail
        ? `Inference service returned an error: ${detail}`
        : "Inference service returned an error. Please try again.";
    default:
      if (anyErr.networkError) {
        return "Could not reach the server. Check your connection and try again.";
      }
      return gql?.message ?? anyErr.message ?? "Prediction failed. Please try again.";
  }
}

function validateField(
  name: keyof FormState,
  value: string
): string | undefined {
  const n = parseFloat(value);
  if (value === "" || isNaN(n)) return "Required";
  if (["judgeSeverity", "attorneyWinRate", "ideologyDistance", "materialityScore"].includes(name)) {
    if (n < 0 || n > 1) return "Must be between 0 and 1";
  }
  if (name === "proceduralMotionCount") {
    if (!Number.isInteger(n) || n < 0 || n > 50)
      return "Must be a whole number between 0 and 50";
  }
}

// S5.8: track which form fields were last populated by the extractor so the
// UI can label them as "Extracted — override?" until the operator edits
// them. The flag is dropped on the first user-driven change to that field.
type ExtractionContext = {
  judgeName: string | null;
  judgeCasesAnalyzed: number | null;
  caseTypeHint: string;
  outcomeFor: string | null;
  /** Provenance for the ideology_distance prefill ("bonica_dime" | "martin_quinn"). */
  ideologySource: string | null;
  /** Release tag the score came from. */
  ideologyRelease: string | null;
  /** Raw score in the source's native scale (DIME [-2,2], MQ ~[-6,6]). */
  ideologyCfscore: number | null;
  /** S8 — MQ term (year). null for DIME. */
  ideologyTerm: number | null;
};

export function IntakeForm() {
  const router = useRouter();
  const apollo = useApolloClient();
  const [form, setForm] = useState<FormState>(INITIAL);
  const [fieldErrors, setFieldErrors] = useState<
    Partial<Record<keyof FormState, string>>
  >({});
  const [submitError, setSubmitError] = useState<string | null>(null);

  // S5.8 extraction state
  const [opinionText, setOpinionText] = useState("");
  const [extractLoading, setExtractLoading] = useState(false);
  const [extractError, setExtractError] = useState<string | null>(null);
  const [extractCtx, setExtractCtx] = useState<ExtractionContext | null>(null);
  const [prefilled, setPrefilled] = useState<Partial<Record<keyof FormState, true>>>({});

  // S6.13 — PDF upload state.  pdfStatus surfaces "Parsing…" while pdfjs-dist
  // is reading the file, and a one-line confirmation ("Loaded N pages") after.
  const [pdfStatus, setPdfStatus] = useState<string | null>(null);
  const [pdfError, setPdfError] = useState<string | null>(null);
  const [pdfLoading, setPdfLoading] = useState(false);
  const pdfInputRef = useRef<HTMLInputElement>(null);

  const [createCase, { loading }] = useMutation<
    CreateCaseData,
    CreateCaseVars
  >(CREATE_CASE);

  function setNumericField(name: keyof FormState) {
    return (e: ChangeEvent<HTMLInputElement>) => {
      setForm((prev) => ({ ...prev, [name]: e.target.value }));
      setFieldErrors((prev) => ({ ...prev, [name]: undefined }));
      // Operator override — drop the prefilled flag for this field.
      setPrefilled((prev) => {
        if (!prev[name]) return prev;
        const { [name]: _, ...rest } = prev;
        return rest;
      });
    };
  }

  /**
   * S6.13 — Pick a PDF, parse it client-side via pdfjs-dist, drop the
   * extracted text into the opinion textarea so the operator can review
   * before running the existing `Extract features` flow.  Scanned PDFs
   * (no extractable text layer) surface a clear "scanned — paste manually"
   * hint until S6.16 ships the Tesseract.js OCR fallback.
   */
  async function handlePdfPick(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    setPdfError(null);
    setPdfStatus(`Parsing ${file.name}…`);
    setPdfLoading(true);
    try {
      const result = await extractTextFromPdf(file);
      if (result.kind === "text") {
        setOpinionText(result.text);
        setPdfStatus(
          `Loaded ${result.pageCount} page${result.pageCount === 1 ? "" : "s"} from ${file.name}`
        );
      } else {
        setPdfStatus(null);
        setPdfError(
          `${file.name} looks like a scanned / image-only PDF (no text layer found across ${result.pageCount} page${result.pageCount === 1 ? "" : "s"}). OCR support is coming — for now, please paste the text manually.`
        );
      }
    } catch (err) {
      setPdfStatus(null);
      const msg =
        err instanceof PdfExtractError
          ? err.message
          : err instanceof Error
            ? err.message
            : "Could not read PDF.";
      setPdfError(msg);
    } finally {
      setPdfLoading(false);
      // Reset the input so picking the same file again still triggers onChange.
      if (pdfInputRef.current) pdfInputRef.current.value = "";
    }
  }

  async function handleExtract() {
    if (!opinionText.trim()) {
      setExtractError("Paste an opinion's plain text first.");
      return;
    }
    setExtractError(null);
    setExtractLoading(true);
    try {
      const res = await apollo.query<ExtractFeaturesData, ExtractFeaturesVars>({
        query: EXTRACT_FEATURES,
        variables: { text: opinionText },
        fetchPolicy: "network-only",
      });
      const ef = res.data?.extractFeatures;
      if (!ef) {
        setExtractError("No suggestions returned.");
        return;
      }
      const nextPrefilled: Partial<Record<keyof FormState, true>> = {};
      setForm((prev) => {
        const next = { ...prev };
        if (ef.judgeSeverity != null) {
          next.judgeSeverity = ef.judgeSeverity.toFixed(2);
          nextPrefilled.judgeSeverity = true;
        }
        // S7 — DIME-derived ideology distance.
        if (ef.ideologyDistance != null) {
          next.ideologyDistance = ef.ideologyDistance.toFixed(2);
          nextPrefilled.ideologyDistance = true;
        }
        if (ef.caseTypeSuggestion === "civil" || ef.caseTypeSuggestion === "criminal" || ef.caseTypeSuggestion === "bankruptcy") {
          next.caseType = ef.caseTypeSuggestion as CaseType;
          nextPrefilled.caseType = true;
        }
        if (ef.jurisdictionSuggestion === "us-federal" || ef.jurisdictionSuggestion === "ca-state" || ef.jurisdictionSuggestion === "nj-state") {
          next.jurisdiction = ef.jurisdictionSuggestion as Jurisdiction;
          nextPrefilled.jurisdiction = true;
        }
        return next;
      });
      setPrefilled(nextPrefilled);
      setExtractCtx({
        judgeName: ef.judgeName,
        judgeCasesAnalyzed: ef.judgeCasesAnalyzed,
        caseTypeHint: ef.caseTypeHint,
        outcomeFor: ef.outcomeFor,
        ideologySource: ef.ideologySource,
        ideologyRelease: ef.ideologyRelease,
        ideologyCfscore: ef.ideologyCfscore,
        ideologyTerm: ef.ideologyTerm,
      });
      setFieldErrors({});
    } catch (e) {
      setExtractError(
        e instanceof Error ? e.message : "Extraction failed. Please try again."
      );
    } finally {
      setExtractLoading(false);
    }
  }

  function validate(): boolean {
    const errs: Partial<Record<keyof FormState, string>> = {};
    const numericFields = [
      "judgeSeverity",
      "attorneyWinRate",
      "ideologyDistance",
      "materialityScore",
      "proceduralMotionCount",
    ] as const;
    for (const f of numericFields) {
      const msg = validateField(f, form[f]);
      if (msg) errs[f] = msg;
    }
    setFieldErrors(errs);
    return Object.keys(errs).length === 0;
  }

  async function handleSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setSubmitError(null);
    if (!validate()) return;

    const input: PredictInput = {
      judgeSeverity: parseFloat(form.judgeSeverity),
      attorneyWinRate: parseFloat(form.attorneyWinRate),
      ideologyDistance: parseFloat(form.ideologyDistance),
      materialityScore: parseFloat(form.materialityScore),
      proceduralMotionCount: parseInt(form.proceduralMotionCount, 10),
      caseType: form.caseType,
      jurisdiction: form.jurisdiction,
    };

    try {
      // S6.8 — forward the prior-opinion text (if the operator pasted any)
      // so the server persists its NLP suggestion alongside these final,
      // possibly hand-edited, values.  Omitted entirely when the field is
      // blank, leaving the server-side nlp_suggestion NULL.
      const trimmedOpinion = opinionText.trim();
      const result = await createCase({
        variables: {
          input,
          ...(trimmedOpinion ? { opinionText: trimmedOpinion } : {}),
        },
      });

      if (result.error || !result.data) {
        setSubmitError(humaniseError(result.error));
        return;
      }

      // S4.4: use the server-assigned UUID from the persisted Case row.
      const { id } = result.data.createCase;
      router.push(`/case/${id}`);
    } catch (err) {
      // Network/transport failure: Apollo's link threw before we got a response.
      setSubmitError(humaniseError(err));
    }
  }

  function fillSample() {
    setForm({ ...INITIAL, ...SAMPLE_DATA });
    setFieldErrors({});
    setSubmitError(null);
  }

  return (
    <Card className="w-full max-w-2xl">
      <CardHeader>
        <CardTitle>Case feature inputs</CardTitle>
        <CardDescription>
          Enter Tier-A/B case features. Tier-C party features are never
          accepted.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {/* S5.8 — Optional prior-opinion extraction panel.  Pasting a prior
            opinion lets the server (extractFeatures GraphQL query) prefill
            fields where the NLP extractor has a confident signal.  Operator
            still owns final values via the standard form below. */}
        <details className="mb-6 rounded-md border border-input bg-muted/30 p-3">
          <summary className="cursor-pointer text-sm font-medium">
            Prefill from a prior opinion (optional)
          </summary>
          <p className="mt-2 text-xs text-muted-foreground">
            Paste an opinion authored by the assigned judge to prefill judge
            severity, case type, and jurisdiction. You can override any
            prefilled value before running prediction.
          </p>
          {/* S6.13 — PDF upload populates the textarea below.  Runs entirely
              client-side so the file never leaves the browser. */}
          <div className="mt-3 flex flex-wrap items-center gap-3">
            <label className="text-xs font-medium">
              <span className="sr-only">Upload a PDF opinion</span>
              <input
                ref={pdfInputRef}
                type="file"
                accept=".pdf,application/pdf"
                onChange={handlePdfPick}
                disabled={pdfLoading}
                aria-label="Upload PDF opinion"
                className={cn(
                  "block text-xs",
                  "file:mr-3 file:rounded-md file:border file:border-input",
                  "file:bg-background file:px-3 file:py-1 file:text-xs",
                  "file:font-medium file:text-foreground hover:file:bg-muted",
                  "file:cursor-pointer disabled:opacity-50"
                )}
              />
            </label>
            {pdfLoading && (
              <span className="text-xs text-muted-foreground" role="status">
                {pdfStatus ?? "Parsing PDF…"}
              </span>
            )}
            {!pdfLoading && pdfStatus && (
              <span className="text-xs text-muted-foreground">{pdfStatus}</span>
            )}
          </div>
          {pdfError && (
            <p role="alert" className="mt-2 text-xs text-destructive">
              {pdfError}
            </p>
          )}
          <p className="mt-2 text-[10px] text-muted-foreground">
            Up to {Math.round(MAX_PDF_BYTES / 1024 / 1024)} MB. Scanned PDFs are not OCR-processed yet (S6.16).
          </p>
          <textarea
            value={opinionText}
            onChange={(e) => setOpinionText(e.target.value)}
            placeholder="Paste opinion plain text…"
            rows={6}
            className={cn(
              "mt-3 block w-full rounded-md border border-input bg-background px-3 py-2",
              "text-sm font-mono shadow-sm outline-none",
              "focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
            )}
            aria-label="Opinion text for feature extraction"
          />
          <div className="mt-2 flex items-center gap-3">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleExtract}
              disabled={extractLoading || !opinionText.trim()}
            >
              {extractLoading ? "Extracting…" : "Extract features"}
            </Button>
            {extractError && (
              <p role="alert" className="text-xs text-destructive">
                {extractError}
              </p>
            )}
          </div>
          {extractCtx && (
            <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
              <dt className="text-muted-foreground">Detected judge</dt>
              <dd>
                {extractCtx.judgeName ?? "—"}
                {extractCtx.judgeCasesAnalyzed != null && extractCtx.judgeName && (
                  <span className="text-muted-foreground">
                    {" "}({extractCtx.judgeCasesAnalyzed} prior opinion
                    {extractCtx.judgeCasesAnalyzed === 1 ? "" : "s"})
                  </span>
                )}
              </dd>
              <dt className="text-muted-foreground">Case-type signal</dt>
              <dd>{extractCtx.caseTypeHint || "—"}</dd>
              <dt className="text-muted-foreground">Disposition</dt>
              <dd>{extractCtx.outcomeFor ?? "unresolved"}</dd>
            </dl>
          )}
        </details>

        <form
          onSubmit={handleSubmit}
          noValidate
          aria-label="New case intake"
        >
          <div className="grid grid-cols-1 gap-5 sm:grid-cols-2">

            {/* Judge Severity */}
            <div className="space-y-1.5">
              <Label htmlFor="judgeSeverity">
                Judge severity
                {prefilled.judgeSeverity && (
                  <span className="ml-2 rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-amber-900">
                    Extracted
                  </span>
                )}
              </Label>
              <Input
                id="judgeSeverity"
                name="judgeSeverity"
                type="number"
                step="0.01"
                min="0"
                max="1"
                placeholder="0.00 – 1.00"
                value={form.judgeSeverity}
                onChange={setNumericField("judgeSeverity")}
                aria-invalid={!!fieldErrors.judgeSeverity}
                aria-describedby="judgeSeverity-help judgeSeverity-error"
              />
              <p id="judgeSeverity-help" className="text-xs text-muted-foreground">
                0 = lenient, 1 = most severe
              </p>
              {fieldErrors.judgeSeverity && (
                <p id="judgeSeverity-error" role="alert" className="text-xs text-destructive">
                  {fieldErrors.judgeSeverity}
                </p>
              )}
            </div>

            {/* Attorney Win Rate */}
            <div className="space-y-1.5">
              <Label htmlFor="attorneyWinRate">Attorney win rate</Label>
              <Input
                id="attorneyWinRate"
                name="attorneyWinRate"
                type="number"
                step="0.01"
                min="0"
                max="1"
                placeholder="0.00 – 1.00"
                value={form.attorneyWinRate}
                onChange={setNumericField("attorneyWinRate")}
                aria-invalid={!!fieldErrors.attorneyWinRate}
                aria-describedby={fieldErrors.attorneyWinRate ? "attorneyWinRate-error" : undefined}
              />
              {fieldErrors.attorneyWinRate && (
                <p id="attorneyWinRate-error" role="alert" className="text-xs text-destructive">
                  {fieldErrors.attorneyWinRate}
                </p>
              )}
            </div>

            {/* Ideology Distance */}
            <div className="space-y-1.5">
              <div className="flex flex-wrap items-center gap-2">
                <Label htmlFor="ideologyDistance">Ideology distance</Label>
                {/* Source-specific ideology badges. Only one renders — the
                    one that actually drove the prefill. MQ is preferred
                    when both are available (voting-record beats
                    campaign-finance proxy). */}
                {prefilled.ideologyDistance && extractCtx?.ideologySource === "martin_quinn" && (
                  <span
                    className="inline-flex items-center gap-1 rounded-full border border-emerald-200 bg-emerald-50 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-emerald-800"
                    title={
                      "Martin-Quinn dynamic ideal-point (voting-record-based). " +
                      (extractCtx.ideologyCfscore != null
                        ? `Posterior mean: ${extractCtx.ideologyCfscore.toFixed(2)} (lower = more liberal). `
                        : "") +
                      (extractCtx.ideologyTerm != null
                        ? `Most recent term with a public score: ${extractCtx.ideologyTerm}. `
                        : "") +
                      `Release: ${extractCtx.ideologyRelease ?? "unknown"}.`
                    }
                  >
                    Martin-Quinn
                    {extractCtx.ideologyTerm != null && (
                      <span className="font-mono normal-case tracking-normal">
                        {extractCtx.ideologyTerm}
                      </span>
                    )}
                  </span>
                )}
                {prefilled.ideologyDistance && extractCtx?.ideologySource === "judicial_common_space" && (
                  <span
                    className="inline-flex items-center gap-1 rounded-full border border-indigo-200 bg-indigo-50 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-indigo-800"
                    title={
                      "Judicial Common Space (Epstein/Martin/Quinn/Segal joint-scaling). " +
                      "Voting-record-based; extends Martin-Quinn beyond SCOTUS to federal Circuit + District judges. " +
                      (extractCtx.ideologyCfscore != null
                        ? `Score: ${extractCtx.ideologyCfscore.toFixed(2)} (range ≈ -1 to +1; lower = more liberal). `
                        : "") +
                      `Release: ${extractCtx.ideologyRelease ?? "unknown"}.`
                    }
                  >
                    JCS
                    {extractCtx.ideologyCfscore != null && (
                      <span className="font-mono normal-case tracking-normal">
                        {extractCtx.ideologyCfscore.toFixed(2)}
                      </span>
                    )}
                  </span>
                )}
                {prefilled.ideologyDistance && extractCtx?.ideologySource === "bonica_dime" && (
                  <span
                    className="inline-flex items-center gap-1 rounded-full border border-blue-200 bg-blue-50 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-blue-700"
                    title={
                      "Bonica DIME campaign-finance ideology proxy. " +
                      (extractCtx.ideologyCfscore != null
                        ? `Raw cfscore: ${extractCtx.ideologyCfscore.toFixed(2)} (range ≈ -2 to +2; lower = more liberal). `
                        : "") +
                      `Release: ${extractCtx.ideologyRelease ?? "unknown"}. ` +
                      "Not a vote-direction prediction."
                    }
                  >
                    Bonica DIME
                    {extractCtx.ideologyCfscore != null && (
                      <span className="font-mono normal-case tracking-normal">
                        {extractCtx.ideologyCfscore.toFixed(2)}
                      </span>
                    )}
                  </span>
                )}
              </div>
              <Input
                id="ideologyDistance"
                name="ideologyDistance"
                type="number"
                step="0.01"
                min="0"
                max="1"
                placeholder="0.00 – 1.00"
                value={form.ideologyDistance}
                onChange={setNumericField("ideologyDistance")}
                aria-invalid={!!fieldErrors.ideologyDistance}
                aria-describedby="ideologyDistance-help ideologyDistance-error"
              />
              <p id="ideologyDistance-help" className="text-xs text-muted-foreground">
                0 = aligned, 1 = maximally opposed
              </p>
              {fieldErrors.ideologyDistance && (
                <p id="ideologyDistance-error" role="alert" className="text-xs text-destructive">
                  {fieldErrors.ideologyDistance}
                </p>
              )}
            </div>

            {/* Materiality Score */}
            <div className="space-y-1.5">
              <Label htmlFor="materialityScore">Materiality score</Label>
              <Input
                id="materialityScore"
                name="materialityScore"
                type="number"
                step="0.01"
                min="0"
                max="1"
                placeholder="0.00 – 1.00"
                value={form.materialityScore}
                onChange={setNumericField("materialityScore")}
                aria-invalid={!!fieldErrors.materialityScore}
                aria-describedby={fieldErrors.materialityScore ? "materialityScore-error" : undefined}
              />
              {fieldErrors.materialityScore && (
                <p id="materialityScore-error" role="alert" className="text-xs text-destructive">
                  {fieldErrors.materialityScore}
                </p>
              )}
            </div>

            {/* Procedural Motion Count */}
            <div className="space-y-1.5">
              <Label htmlFor="proceduralMotionCount">Procedural motions filed</Label>
              <Input
                id="proceduralMotionCount"
                name="proceduralMotionCount"
                type="number"
                step="1"
                min="0"
                max="50"
                placeholder="0 – 50"
                value={form.proceduralMotionCount}
                onChange={setNumericField("proceduralMotionCount")}
                aria-invalid={!!fieldErrors.proceduralMotionCount}
                aria-describedby={fieldErrors.proceduralMotionCount ? "proceduralMotionCount-error" : undefined}
              />
              {fieldErrors.proceduralMotionCount && (
                <p id="proceduralMotionCount-error" role="alert" className="text-xs text-destructive">
                  {fieldErrors.proceduralMotionCount}
                </p>
              )}
            </div>

            {/* Case Type */}
            <div className="space-y-1.5">
              <Label htmlFor="caseType">
                Case type
                {prefilled.caseType && (
                  <span className="ml-2 rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-amber-900">
                    Extracted
                  </span>
                )}
              </Label>
              <select
                id="caseType"
                name="caseType"
                className={selectCn}
                value={form.caseType}
                onChange={(e) => {
                  setForm((prev) => ({
                    ...prev,
                    caseType: e.target.value as CaseType,
                  }));
                  setPrefilled((prev) => {
                    if (!prev.caseType) return prev;
                    const { caseType: _, ...rest } = prev;
                    return rest;
                  });
                }}
              >
                {CASE_TYPES.map((ct) => (
                  <option key={ct.value} value={ct.value}>
                    {ct.label}
                  </option>
                ))}
              </select>
            </div>

            {/* Jurisdiction */}
            <div className="space-y-1.5 sm:col-span-2">
              <Label htmlFor="jurisdiction">
                Jurisdiction
                {prefilled.jurisdiction && (
                  <span className="ml-2 rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-amber-900">
                    Extracted
                  </span>
                )}
              </Label>
              <select
                id="jurisdiction"
                name="jurisdiction"
                className={selectCn}
                value={form.jurisdiction}
                onChange={(e) => {
                  setForm((prev) => ({
                    ...prev,
                    jurisdiction: e.target.value as Jurisdiction,
                  }));
                  setPrefilled((prev) => {
                    if (!prev.jurisdiction) return prev;
                    const { jurisdiction: _, ...rest } = prev;
                    return rest;
                  });
                }}
              >
                {JURISDICTIONS.map((j) => (
                  <option key={j.value} value={j.value}>
                    {j.label}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {/* Submit error */}
          {submitError && (
            <p
              role="alert"
              aria-live="assertive"
              className="mt-4 text-sm text-destructive"
            >
              {submitError}
            </p>
          )}

          <div className="mt-6 flex items-center gap-4">
            <Button type="submit" size="lg" disabled={loading}>
              {loading ? "Predicting…" : "Run prediction"}
            </Button>

            {/* Developer convenience — hidden in production builds. */}
            {process.env.NODE_ENV !== "production" && (
              <button
                type="button"
                onClick={fillSample}
                className="text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
              >
                Fill with sample case
              </button>
            )}
          </div>
        </form>
      </CardContent>
    </Card>
  );
}
