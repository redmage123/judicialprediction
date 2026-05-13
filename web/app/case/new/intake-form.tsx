"use client";

// Sprint-3 follow-up: replaced sessionStorage + client UUID with createCase mutation
// that persists the case server-side and returns a real server UUID (S4.4 / JP-58).

import { useState, type FormEvent, type ChangeEvent } from "react";
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
      const result = await createCase({ variables: { input } });

      if (result.error || !result.data) {
        setSubmitError(
          result.error?.message ?? "Prediction failed. Please try again."
        );
        return;
      }

      // S4.4: use the server-assigned UUID from the persisted Case row.
      const { id } = result.data.createCase;
      router.push(`/case/${id}`);
    } catch {
      setSubmitError(
        "Unable to reach the gateway. Please try again."
      );
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
              <Label htmlFor="ideologyDistance">Ideology distance</Label>
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
