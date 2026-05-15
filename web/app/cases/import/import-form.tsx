"use client";

/**
 * S6.14 — CSV bulk-import client island.
 *
 * Parses the operator's CSV with papaparse, surfaces a first-row preview
 * so they can sanity-check the column mapping, then calls the
 * `importCases` GraphQL mutation with the parsed rows.  Returns a
 * per-row pass/fail summary that links each succeeded row to its
 * created case.
 *
 * No BFF route — Apollo's existing JWT-cookie wiring handles auth and
 * the gateway's per-row error handling is precisely what we render.
 */

import { useState, type ChangeEvent } from "react";
import Link from "next/link";
import Papa from "papaparse";
import { useMutation } from "@apollo/client/react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { cn } from "@/lib/utils";
import {
  IMPORT_CASES,
  IMPORT_CSV_HEADERS,
  IMPORT_CSV_OPTIONAL_HEADERS,
  MAX_IMPORT_ROWS,
  type ImportCaseRowInput,
  type ImportCasesData,
  type ImportCasesVars,
  type ImportCasesResult,
} from "@/lib/queries/import";
import { CASE_TYPES, type CaseType } from "@/lib/case-types";
import { JURISDICTIONS, type Jurisdiction } from "@/lib/jurisdictions";

const ALL_HEADERS = [...IMPORT_CSV_HEADERS, ...IMPORT_CSV_OPTIONAL_HEADERS];
const PREVIEW_ROW_LIMIT = 5;

type ValidationError = { row: number; message: string };

interface ParsedCsv {
  rows: ImportCaseRowInput[];
  errors: ValidationError[];
}

/**
 * Parse and validate the CSV body.  Errors are collected per-row so the
 * operator sees every column-mapping issue at once rather than one at a
 * time.  Header check is strict — extra columns are allowed but missing
 * required columns abort before any row work.
 */
function parseAndValidate(csvText: string): ParsedCsv {
  const parsed = Papa.parse<Record<string, string>>(csvText, {
    header: true,
    skipEmptyLines: true,
  });

  const fields = parsed.meta.fields ?? [];
  const missing = IMPORT_CSV_HEADERS.filter((h) => !fields.includes(h));
  if (missing.length > 0) {
    return {
      rows: [],
      errors: [
        {
          row: 0,
          message: `CSV is missing required column(s): ${missing.join(", ")}. Required: ${IMPORT_CSV_HEADERS.join(", ")}.`,
        },
      ],
    };
  }

  const rows: ImportCaseRowInput[] = [];
  const errors: ValidationError[] = [];

  parsed.data.forEach((raw, idx) => {
    const rowNum = idx + 2; // header is line 1, first data row is line 2
    const judge = Number(raw.judge_severity);
    const att = Number(raw.attorney_win_rate);
    const ideo = Number(raw.ideology_distance);
    const mat = Number(raw.materiality_score);
    const motions = Number(raw.procedural_motion_count);
    const caseType = (raw.case_type ?? "").trim();
    const jurisdiction = (raw.jurisdiction ?? "").trim();
    const opinion = (raw.opinion_text ?? "").trim();

    for (const [label, value, min, max] of [
      ["judge_severity", judge, 0, 1],
      ["attorney_win_rate", att, 0, 1],
      ["ideology_distance", ideo, 0, 1],
      ["materiality_score", mat, 0, 1],
      ["procedural_motion_count", motions, 0, 50],
    ] as const) {
      if (Number.isNaN(value)) {
        errors.push({ row: rowNum, message: `${label} is not a number` });
      } else if (value < min || value > max) {
        errors.push({ row: rowNum, message: `${label} ${value} out of range [${min}, ${max}]` });
      }
    }
    if (!CASE_TYPES.some((c) => c.value === caseType)) {
      errors.push({ row: rowNum, message: `case_type "${caseType}" is not one of ${CASE_TYPES.map((c) => c.value).join(", ")}` });
    }
    if (!JURISDICTIONS.some((j) => j.value === jurisdiction)) {
      errors.push({ row: rowNum, message: `jurisdiction "${jurisdiction}" is not one of ${JURISDICTIONS.map((j) => j.value).join(", ")}` });
    }

    rows.push({
      judgeSeverity: judge,
      attorneyWinRate: att,
      ideologyDistance: ideo,
      materialityScore: mat,
      proceduralMotionCount: motions,
      caseType: caseType as CaseType,
      jurisdiction: jurisdiction as Jurisdiction,
      ...(opinion ? { opinionText: opinion } : {}),
    });
  });

  return { rows, errors };
}

export function ImportForm() {
  const [fileName, setFileName] = useState<string | null>(null);
  const [parsed, setParsed] = useState<ParsedCsv | null>(null);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [result, setResult] = useState<ImportCasesResult | null>(null);

  const [importCases, { loading }] = useMutation<
    ImportCasesData,
    ImportCasesVars
  >(IMPORT_CASES);

  async function handleFile(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    setFileName(file.name);
    setSubmitError(null);
    setResult(null);
    try {
      const text = await file.text();
      const p = parseAndValidate(text);
      setParsed(p);
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : "Could not read file.");
      setParsed(null);
    }
  }

  async function handleSubmit() {
    if (!parsed || parsed.errors.length > 0 || parsed.rows.length === 0) return;
    setSubmitError(null);
    try {
      const res = await importCases({ variables: { rows: parsed.rows } });
      if (res.error || !res.data) {
        setSubmitError(res.error?.message ?? "Import failed.");
        return;
      }
      setResult(res.data.importCases);
    } catch (e) {
      setSubmitError(e instanceof Error ? e.message : "Import failed.");
    }
  }

  const tooManyRows = !!parsed && parsed.rows.length > MAX_IMPORT_ROWS;
  const hasBlockingErrors =
    !!parsed && (parsed.errors.length > 0 || tooManyRows);

  return (
    <Card className="w-full">
      <CardHeader>
        <CardTitle>Upload CSV</CardTitle>
        <CardDescription>
          Required headers: {IMPORT_CSV_HEADERS.join(", ")}. Optional:{" "}
          {IMPORT_CSV_OPTIONAL_HEADERS.join(", ")}. Up to {MAX_IMPORT_ROWS} rows
          per request.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <label className="block text-sm">
          <span className="sr-only">Upload cases CSV</span>
          <input
            type="file"
            accept=".csv,text/csv"
            onChange={handleFile}
            aria-label="Upload cases CSV"
            disabled={loading}
            className={cn(
              "block text-sm",
              "file:mr-3 file:rounded-md file:border file:border-input",
              "file:bg-background file:px-3 file:py-1 file:text-xs",
              "file:font-medium file:text-foreground hover:file:bg-muted",
              "file:cursor-pointer disabled:opacity-50"
            )}
          />
        </label>
        {fileName && (
          <p className="text-xs text-muted-foreground">
            Loaded {fileName} — {parsed?.rows.length ?? 0} row
            {parsed?.rows.length === 1 ? "" : "s"}
          </p>
        )}

        {parsed && parsed.errors.length > 0 && (
          <div role="alert" className="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-xs">
            <p className="font-medium text-destructive">
              {parsed.errors.length} validation error
              {parsed.errors.length === 1 ? "" : "s"}:
            </p>
            <ul className="mt-1 list-disc pl-5">
              {parsed.errors.slice(0, 10).map((err, i) => (
                <li key={i}>
                  row {err.row}: {err.message}
                </li>
              ))}
              {parsed.errors.length > 10 && (
                <li className="text-muted-foreground">…and {parsed.errors.length - 10} more</li>
              )}
            </ul>
          </div>
        )}

        {tooManyRows && (
          <p role="alert" className="text-xs text-destructive">
            {parsed!.rows.length} rows exceeds the {MAX_IMPORT_ROWS}-row limit.
            Split the file into batches.
          </p>
        )}

        {parsed && parsed.rows.length > 0 && (
          <div className="overflow-x-auto rounded-md border border-input">
            <table className="w-full text-xs">
              <caption className="px-3 py-2 text-left text-xs text-muted-foreground">
                Preview — first {Math.min(PREVIEW_ROW_LIMIT, parsed.rows.length)} of {parsed.rows.length} rows
              </caption>
              <thead className="bg-muted/30">
                <tr>
                  {ALL_HEADERS.map((h) => (
                    <th key={h} className="px-2 py-1 text-left font-medium">{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {parsed.rows.slice(0, PREVIEW_ROW_LIMIT).map((r, i) => (
                  <tr key={i} className="border-t border-input/40">
                    <td className="px-2 py-1">{r.judgeSeverity}</td>
                    <td className="px-2 py-1">{r.attorneyWinRate}</td>
                    <td className="px-2 py-1">{r.ideologyDistance}</td>
                    <td className="px-2 py-1">{r.materialityScore}</td>
                    <td className="px-2 py-1">{r.proceduralMotionCount}</td>
                    <td className="px-2 py-1">{r.caseType}</td>
                    <td className="px-2 py-1">{r.jurisdiction}</td>
                    <td className="px-2 py-1 max-w-[200px] truncate text-muted-foreground">
                      {r.opinionText ? `${r.opinionText.slice(0, 60)}…` : "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        <div className="flex items-center gap-3">
          <Button
            type="button"
            onClick={handleSubmit}
            disabled={
              loading || !parsed || parsed.rows.length === 0 || hasBlockingErrors
            }
          >
            {loading ? "Importing…" : `Import ${parsed?.rows.length ?? 0} case${parsed?.rows.length === 1 ? "" : "s"}`}
          </Button>
          {submitError && (
            <p role="alert" className="text-xs text-destructive">{submitError}</p>
          )}
        </div>

        {result && (
          <div className="rounded-md border border-input bg-muted/20 p-3">
            <p className="text-sm font-medium">
              Import complete — {result.succeeded} succeeded, {result.failed} failed (of {result.total}).
            </p>
            <table className="mt-3 w-full text-xs">
              <thead className="bg-muted/30">
                <tr>
                  <th className="px-2 py-1 text-left">row</th>
                  <th className="px-2 py-1 text-left">status</th>
                  <th className="px-2 py-1 text-left">link / error</th>
                </tr>
              </thead>
              <tbody>
                {result.results.map((r) => (
                  <tr key={r.rowIndex} className="border-t border-input/40">
                    <td className="px-2 py-1">{r.rowIndex + 2 /* CSV line, header = 1 */}</td>
                    <td className={cn("px-2 py-1", r.ok ? "text-green-700" : "text-destructive")}>
                      {r.ok ? "ok" : "failed"}
                    </td>
                    <td className="px-2 py-1">
                      {r.ok && r.caseId ? (
                        <Link href={`/case/${r.caseId}`} className="text-primary underline">
                          View case
                        </Link>
                      ) : (
                        <span className="text-muted-foreground">{r.error ?? "—"}</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
