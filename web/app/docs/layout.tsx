import type { Metadata } from "next";
import { DocsSidebar } from "./docs-sidebar";

export const metadata: Metadata = {
  title: "Documentation — JudicialPredict",
};

export default function DocsLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="mx-auto grid w-full max-w-6xl grid-cols-[12rem_1fr] gap-8 px-6 py-10">
      <DocsSidebar />
      <main className="min-w-0">{children}</main>
    </div>
  );
}
