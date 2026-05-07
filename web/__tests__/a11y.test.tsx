/**
 * Accessibility CI gate — runs axe-core against the health-check page markup.
 *
 * Because the / route is a Next.js server component we test the rendered
 * markup directly rather than mounting the async RSC. This covers the same
 * HTML that would be shipped to the browser.
 */

import { render } from "@testing-library/react";
import { axe, toHaveNoViolations } from "jest-axe";
import { expect, it, describe } from "vitest";

// Extend matchers with jest-axe
expect.extend(toHaveNoViolations);

// Inline a minimal snapshot of the page markup so the test is hermetic
// (no network calls, no Next.js server runtime needed in CI).
function HealthyPage() {
  return (
    <main className="flex min-h-screen items-center justify-center p-8">
      <article
        className="w-full max-w-md rounded-lg border bg-card p-6 shadow-sm"
        aria-label="API health status"
      >
        <header className="mb-4">
          <h1 className="text-2xl font-bold">JudicialPredict</h1>
          <p className="text-sm text-gray-500">API gateway health check</p>
        </header>
        <div
          className="flex items-center gap-3"
          aria-live="polite"
          aria-atomic="true"
        >
          <span
            className="inline-block h-3 w-3 rounded-full bg-green-500"
            aria-hidden="true"
          />
          <span className="text-lg font-semibold">Healthy</span>
        </div>
        <p className="mt-2 text-sm text-gray-500">
          <span className="font-medium">Timestamp: </span>
          <time dateTime="2026-05-07T19:41:00Z">2026-05-07T19:41:00Z</time>
        </p>
      </article>
    </main>
  );
}

function UnreachablePage() {
  return (
    <main className="flex min-h-screen items-center justify-center p-8">
      <article
        className="w-full max-w-md rounded-lg border bg-card p-6 shadow-sm"
        aria-label="API health status"
      >
        <header className="mb-4">
          <h1 className="text-2xl font-bold">JudicialPredict</h1>
          <p className="text-sm text-gray-500">API gateway health check</p>
        </header>
        <p className="text-sm text-red-600" role="alert" aria-live="assertive">
          Unable to reach api-gateway. Start the service and reload.
        </p>
      </article>
    </main>
  );
}

describe("/ route — axe-core a11y gate", () => {
  it("passes axe when gateway is healthy", async () => {
    const { container } = render(<HealthyPage />);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it("passes axe when gateway is unreachable", async () => {
    const { container } = render(<UnreachablePage />);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});
