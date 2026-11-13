"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/utils";

const SECTIONS = [
  { href: "/docs/howto", label: "How-to" },
  { href: "/docs/users-guide", label: "Users guide" },
  { href: "/docs/faq", label: "FAQ" },
];

export function DocsSidebar() {
  const pathname = usePathname();

  return (
    <aside className="border-r pr-4">
      <p className="mb-3 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
        Documentation
      </p>
      <nav aria-label="Documentation sections" className="flex flex-col gap-1 text-sm">
        {SECTIONS.map((s) => {
          const isActive =
            pathname === s.href || pathname.startsWith(s.href + "/");
          return (
            <Link
              key={s.href}
              href={s.href}
              aria-current={isActive ? "page" : undefined}
              className={cn(
                "rounded-md px-2 py-1 transition-colors",
                isActive
                  ? "bg-muted font-semibold text-foreground"
                  : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
              )}
            >
              {s.label}
            </Link>
          );
        })}
      </nav>
    </aside>
  );
}
