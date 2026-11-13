import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "How-to — JudicialPredict",
};

export default function HowToPage() {
  return (
    <article className="prose prose-slate max-w-none text-sm leading-relaxed">
      <h1 className="text-3xl font-bold tracking-tight">How-to</h1>
      <p className="mt-2 text-muted-foreground">
        Task-focused recipes for getting common workflows done in
        JudicialPredict.
      </p>
      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Coming soon</h2>
        <p>
          Step-by-step guides for the most-used operator workflows will land
          here. Topics planned: create a new case, run a bulk import, read a
          calibration plot, export an audit report.
        </p>
      </section>
    </article>
  );
}
