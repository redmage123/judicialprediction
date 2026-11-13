"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/utils";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

type NavLeaf = { href: string; label: string };
type NavGroup = { label: string; basePath: string; children: NavLeaf[] };
type NavItem = NavLeaf | NavGroup;

function isGroup(item: NavItem): item is NavGroup {
  return "children" in item;
}

const NAV_ITEMS: NavItem[] = [
  { href: "/cases", label: "Cases" },
  { href: "/case/new", label: "New case" },
  { href: "/cases/import", label: "Bulk import" },
  { href: "/audit", label: "Audit log" },
  {
    label: "Documentation",
    basePath: "/docs",
    children: [
      { href: "/docs/howto", label: "How-to" },
      { href: "/docs/users-guide", label: "Users guide" },
      { href: "/docs/faq", label: "FAQ" },
    ],
  },
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
        if (isGroup(item)) {
          const groupActive =
            pathname === item.basePath ||
            pathname.startsWith(item.basePath + "/");
          return (
            <DropdownMenu key={item.basePath}>
              <DropdownMenuTrigger
                aria-current={groupActive ? "page" : undefined}
                className={cn(
                  "rounded-md px-3 py-1.5 transition-colors outline-none",
                  "focus-visible:ring-[3px] focus-visible:ring-ring/50",
                  groupActive
                    ? "bg-muted font-semibold text-foreground"
                    : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                )}
              >
                {item.label}
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start">
                {item.children.map((child) => {
                  const childActive =
                    pathname === child.href ||
                    pathname.startsWith(child.href + "/");
                  return (
                    <DropdownMenuItem key={child.href} asChild>
                      <Link
                        href={child.href}
                        aria-current={childActive ? "page" : undefined}
                        className={cn(
                          "w-full cursor-pointer",
                          childActive && "font-semibold",
                        )}
                      >
                        {child.label}
                      </Link>
                    </DropdownMenuItem>
                  );
                })}
              </DropdownMenuContent>
            </DropdownMenu>
          );
        }

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
                : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
            )}
          >
            {item.label}
          </Link>
        );
      })}
    </nav>
  );
}
