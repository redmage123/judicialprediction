import type { Metadata } from "next";
import { ResultsView } from "./results-view";

// force-dynamic: data lives in sessionStorage (per-user, client-side) so there is
// nothing to statically generate or cache at the server level.
export const dynamic = "force-dynamic";

type Props = {
  params: Promise<{ id: string }>;
};

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { id } = await params;
  const prefix = id.slice(0, 8);
  return {
    title: `Case ${prefix} — JudicialPredict`,
  };
}

export default async function CasePage({ params }: Props) {
  const { id } = await params;
  // ResultsView is the client island; it reads sessionStorage on mount.
  return <ResultsView caseId={id} />;
}
