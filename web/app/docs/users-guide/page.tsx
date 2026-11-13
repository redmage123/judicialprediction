import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Users guide — JudicialPredict",
};

export default function UsersGuidePage() {
  return (
    <article className="prose prose-slate max-w-none text-sm leading-relaxed">
      <h1 className="text-3xl font-bold tracking-tight">Users guide</h1>
      <p className="mt-2 text-muted-foreground">
        Conceptual overview of how JudicialPredict produces its predictions
        and how to interpret each piece of the workspace UI.
      </p>
      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Coming soon</h2>
        <p>
          Long-form documentation covering the model lifecycle, the
          calibration story, what each feature group contributes, and how
          operators should think about uncertainty in the recommended course
          of action.
        </p>
      </section>
    </article>
  );
}
