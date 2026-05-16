"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/utils";

const NAV_ITEMS: { href: string; label: string }[] = [
  { href: "/cases", label: "Cases" },
  { href: "/case/new", label: "New case" },
  { href: "/cases/import", label: "Bulk import" },
  { href: "/audit", label: "Audit log" },
];

/**
 * Top primary nav. Renders the active route with a bottom underline + bolder
 * weight so operators can see where they are. `aria-current="page"` is set on
 * the active link for screen-reader users.
 */
export function PrimaryNav() {
  const pathname = usePathname();

  return (
    <nav className="flex items-center gap-1 text-sm" aria-label="Primary">
      {NAV_ITEMS.map((item) => {
        const isActive =
          pathname === item.href ||
          (item.href !== "/" && pathname.startsWith(item.href + "/"));
        return (
          <Link
            key={item.href}
            href={item.href}
            aria-current={isActive ? "page" : undefined}
            className={cn(
              "rounded-md px-3 py-1.5 transition-colors",
              isActive
                ? "bg-muted font-semibold text-foreground"
                : "text-muted-foreground hover:bg-muted/60 hover:text-foreground"
            )}
          >
            {item.label}
          </Link>
        );
      })}
    </nav>
  );
}
