"use client";

// Sprint-3 follow-up: replaced sessionStorage + client UUID with createCase mutation
// that persists the case server-side and returns a real server UUID (S4.4 / JP-58).

import { useState, type FormEvent, type ChangeEvent } from "react";
import { useRouter } from "next/navigation";
import { useMutation } from "@apollo/client/react";
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
  type PredictInput,
  type CreateCaseData,
  type CreateCaseVars,
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

export function IntakeForm() {
  const router = useRouter();
  const [form, setForm] = useState<FormState>(INITIAL);
  const [fieldErrors, setFieldErrors] = useState<
    Partial<Record<keyof FormState, string>>
  >({});
  const [submitError, setSubmitError] = useState<string | null>(null);

  const [createCase, { loading }] = useMutation<
    CreateCaseData,
    CreateCaseVars
  >(CREATE_CASE);

  function setNumericField(name: keyof FormState) {
    return (e: ChangeEvent<HTMLInputElement>) => {
      setForm((prev) => ({ ...prev, [name]: e.target.value }));
      setFieldErrors((prev) => ({ ...prev, [name]: undefined }));
    };
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
        <form
          onSubmit={handleSubmit}
          noValidate
          aria-label="New case intake"
        >
          <div className="grid grid-cols-1 gap-5 sm:grid-cols-2">

            {/* Judge Severity */}
            <div className="space-y-1.5">
              <Label htmlFor="judgeSeverity">Judge severity</Label>
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
              <Label htmlFor="caseType">Case type</Label>
              <select
                id="caseType"
                name="caseType"
                className={selectCn}
                value={form.caseType}
                onChange={(e) =>
                  setForm((prev) => ({
                    ...prev,
                    caseType: e.target.value as CaseType,
                  }))
                }
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
              <Label htmlFor="jurisdiction">Jurisdiction</Label>
              <select
                id="jurisdiction"
                name="jurisdiction"
                className={selectCn}
                value={form.jurisdiction}
                onChange={(e) =>
                  setForm((prev) => ({
                    ...prev,
                    jurisdiction: e.target.value as Jurisdiction,
                  }))
                }
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
            <Button type="submit" disabled={loading}>
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
