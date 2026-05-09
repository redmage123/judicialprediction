import type { Metadata } from "next";
import { IntakeForm } from "./intake-form";

export const metadata: Metadata = {
  title: "New case — JudicialPredict",
  description:
    "Enter Tier-A/B case features to generate a predicted case outcome.",
};

/**
 * /case/new — server component shell.
 *
 * The IntakeForm island handles all client-side state, Apollo mutation,
 * and post-predict navigation.  This shell provides the page frame,
 * metadata, and introductory copy.
 */
export default function NewCasePage() {
  return (
    <main className="flex min-h-screen flex-col items-center px-4 py-12">
      <div className="w-full max-w-2xl space-y-6">
        <div className="space-y-1">
          <h1 className="text-2xl font-bold tracking-tight">New case</h1>
          <p className="text-sm text-muted-foreground">
            Enter Tier-A/B case features. Tier-C party features are never
            accepted.
          </p>
        </div>

        <IntakeForm />
      </div>
    </main>
  );
}
