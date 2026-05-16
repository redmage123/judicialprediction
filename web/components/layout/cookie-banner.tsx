"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";

/**
 * Bare-minimum GDPR cookie banner.
 *
 * Stores the operator's choice in localStorage. We do not ship analytics or
 * marketing cookies yet, so "accept" and "reject" currently do the same thing
 * (record the choice and hide the banner). They are surfaced separately and
 * given equal visual weight per CNIL/EDPB symmetry guidance so that adding
 * tracking later requires only flipping the consent gate, not the UI.
 *
 * localStorage access is guarded — older Safari + privacy modes throw on
 * read/write and an uncaught throw here would crash the SSR hydration boundary.
 */
const STORAGE_KEY = "jp.cookie-consent.v1";

type Choice = "accepted" | "rejected" | null;

function readChoice(): Choice {
  try {
    const v = window.localStorage.getItem(STORAGE_KEY);
    return v === "accepted" || v === "rejected" ? v : null;
  } catch {
    return null;
  }
}

function writeChoice(choice: Exclude<Choice, null>): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, choice);
  } catch {
    // Storage blocked — let the banner re-appear next visit. Better than crashing.
  }
}

export function CookieBanner() {
  // Start hidden on first render to avoid an SSR/CSR mismatch flash.
  const [open, setOpen] = useState(false);

  useEffect(() => {
    setOpen(readChoice() === null);
  }, []);

  if (!open) return null;

  const close = (choice: Exclude<Choice, null>) => {
    writeChoice(choice);
    setOpen(false);
  };

  return (
    <div
      role="dialog"
      aria-labelledby="cookie-banner-title"
      aria-describedby="cookie-banner-body"
      className="fixed inset-x-0 bottom-0 z-50 border-t bg-background/95 px-6 py-4 shadow-lg backdrop-blur"
    >
      <div className="mx-auto flex max-w-5xl flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-1">
          <p id="cookie-banner-title" className="text-sm font-semibold">
            We use a session cookie to keep you signed in.
          </p>
          <p id="cookie-banner-body" className="text-xs text-muted-foreground">
            JudicialPredict stores a single, encrypted session cookie that is
            strictly necessary for authentication. We do not run advertising or
            third-party tracking. See our{" "}
            <Link href="/privacy" className="underline underline-offset-2">
              Privacy Policy
            </Link>{" "}
            and{" "}
            <Link href="/cookies" className="underline underline-offset-2">
              Cookie Policy
            </Link>
            .
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button variant="outline" size="sm" onClick={() => close("rejected")}>
            Reject non-essential
          </Button>
          <Button size="sm" onClick={() => close("accepted")}>
            Accept
          </Button>
        </div>
      </div>
    </div>
  );
}
