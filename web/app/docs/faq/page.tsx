import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "FAQ — JudicialPredict",
};

export default function FaqPage() {
  return (
    <article className="prose prose-slate max-w-none text-sm leading-relaxed">
      <h1 className="text-3xl font-bold tracking-tight">FAQ</h1>
      <p className="mt-2 text-muted-foreground">
        Short answers to the questions operators ask most often.
      </p>
      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Coming soon</h2>
        <p>
          Entries will cover supported jurisdictions, what the calibration
          numbers mean, how often the model is retrained, and where to file
          feedback or bug reports.
        </p>
      </section>
    </article>
  );
}
