// S6.14 — /cases/import page shell.
//
// The page is a server component (metadata + intro copy) that hosts the
// ImportForm client island.  CSV parsing happens entirely client-side via
// papaparse so operators get a preview of the first rows before they
// commit to the GraphQL importCases mutation.

import type { Metadata } from "next";
import { ImportForm } from "./import-form";

export const metadata: Metadata = {
  title: "Bulk import — JudicialPredict",
  description:
    "Upload a CSV of cases (Tier-A/B features + optional opinion text) to predict outcomes in one request.",
};

export default function ImportCasesPage() {
  return (
    <main className="flex min-h-screen flex-col items-center px-4 py-12">
      <div className="w-full max-w-3xl space-y-6">
        <div className="space-y-1">
          <h1 className="text-2xl font-bold tracking-tight">Bulk import</h1>
          <p className="text-sm text-muted-foreground">
            Upload a CSV with the seven Tier-A/B features (plus an optional
            opinion text column) to predict up to 50 cases in one request.
            Each row runs the same ML + decision-arith pipeline as the New
            Case form.
          </p>
        </div>
        <ImportForm />
      </div>
    </main>
  );
}
