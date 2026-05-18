"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";

/**
 * GDPR cookie banner — compact mode.
 *
 * Stores the operator's choice in localStorage. We don't ship analytics
 * or marketing cookies yet, so accept and reject record the same thing
 * (we surface them separately and at equal weight per CNIL/EDPB
 * symmetry — once we add a non-essential cookie the consent gate is
 * already wired).
 *
 * localStorage access is guarded — older Safari + privacy modes throw on
 * read/write and an uncaught throw here would crash the SSR hydration
 * boundary.
 *
 * Sprint 13 audit (2026-05-17): the old banner was 3 lines tall + ~120px
 * which overlapped form fields mid-page (Jurisdiction dropdown on
 * /case/new, last rows of the cases dashboard) even with body
 * padding-bottom. Rewrote as a compact one-row bar (~56px) with a
 * collapsible "Details" disclosure that holds the longer explanation.
 * Buttons stay equal weight (Reject non-essential / Accept) for
 * regulator-defensible symmetry.
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
    // Storage blocked — let the banner re-appear next visit.  Better than
    // crashing.
  }
}

export function CookieBanner() {
  // Start hidden on first render to avoid an SSR/CSR mismatch flash.
  const [open, setOpen] = useState(false);
  const [showDetails, setShowDetails] = useState(false);

  useEffect(() => {
    setOpen(readChoice() === null);
  }, []);

  // Push page-bottom padding so the banner doesn't overlap the last
  // rows of long pages. The bottom padding tracks the actual rendered
  // banner height: compact (~56px) → small padding, expanded (~140px) →
  // larger padding.
  useEffect(() => {
    if (typeof document === "undefined") return;
    if (open) {
      document.body.classList.add("has-cookie-banner");
      if (showDetails) {
        document.body.classList.add("has-cookie-banner-expanded");
      } else {
        document.body.classList.remove("has-cookie-banner-expanded");
      }
      return () => {
        document.body.classList.remove("has-cookie-banner");
        document.body.classList.remove("has-cookie-banner-expanded");
      };
    }
    document.body.classList.remove("has-cookie-banner");
    document.body.classList.remove("has-cookie-banner-expanded");
  }, [open, showDetails]);

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
      className="fixed inset-x-0 bottom-0 z-50 border-t bg-background/95 px-6 py-2.5 shadow-lg backdrop-blur"
    >
      <div className="mx-auto flex max-w-5xl flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <p id="cookie-banner-title" className="text-sm">
            <span className="font-semibold">Session cookie</span>{" "}
            <span className="text-muted-foreground">
              keeps you signed in. No tracking.
            </span>
          </p>
          <button
            type="button"
            onClick={() => setShowDetails((v) => !v)}
            aria-expanded={showDetails}
            aria-controls="cookie-banner-body"
            className="text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
          >
            {showDetails ? "Hide details" : "Details"}
          </button>
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
      {showDetails && (
        <div
          id="cookie-banner-body"
          className="mx-auto mt-3 max-w-5xl border-t pt-3 text-xs text-muted-foreground"
        >
          JudicialPredict stores a single, encrypted session cookie
          (<code className="rounded bg-muted px-1 font-mono">jp_session</code>)
          that is strictly necessary for authentication. We do not run
          advertising or third-party tracking. See our{" "}
          <Link href="/privacy" className="underline underline-offset-2">
            Privacy Policy
          </Link>{" "}
          and{" "}
          <Link href="/cookies" className="underline underline-offset-2">
            Cookie Policy
          </Link>
          .
        </div>
      )}
    </div>
  );
}
