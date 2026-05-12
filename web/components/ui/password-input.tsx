"use client";

import * as React from "react";
import { Eye, EyeOff } from "lucide-react";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

/**
 * PasswordInput — a password field with a show/hide eye toggle.
 *
 * Accepts the same props as the underlying <Input>.  The `type` prop is
 * managed internally (forced to "password" / "text"); callers should NOT pass
 * a `type` themselves.  The toggle button is keyboard-focusable, labelled,
 * and updates aria-pressed for assistive tech.
 *
 * Used everywhere a password is entered: login, reset-password (new +
 * confirm fields), and any future flow that asks for a secret.
 */
type PasswordInputProps = Omit<React.ComponentProps<typeof Input>, "type">;

export function PasswordInput({ className, ...props }: PasswordInputProps) {
  const [revealed, setRevealed] = React.useState(false);
  const label = revealed ? "Hide password" : "Show password";

  return (
    <div className="relative">
      <Input
        {...props}
        type={revealed ? "text" : "password"}
        // Reserve room on the right edge so the eye button doesn't sit on
        // top of the typed characters.
        className={cn("pr-10", className)}
      />
      <button
        type="button"
        onClick={() => setRevealed((r) => !r)}
        aria-label={label}
        aria-pressed={revealed}
        title={label}
        tabIndex={0}
        className={cn(
          "absolute inset-y-0 right-0 flex h-full w-10 items-center justify-center",
          "text-muted-foreground hover:text-foreground",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-md"
        )}
      >
        {revealed ? (
          <EyeOff aria-hidden className="h-4 w-4" />
        ) : (
          <Eye aria-hidden className="h-4 w-4" />
        )}
      </button>
    </div>
  );
}
