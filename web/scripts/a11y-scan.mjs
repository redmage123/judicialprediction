#!/usr/bin/env node
/**
 * a11y-scan.mjs — End-to-end axe-core accessibility scan (CI gate).
 *
 * Routes scanned:   /login, /case/new
 * Impact threshold: serious + critical only.
 *                   minor/moderate are deferred to Sprint 4.
 *
 * /case/[id] is intentionally excluded: the results view requires a
 * prediction pre-stashed in sessionStorage by the intake form. That
 * route is covered by per-page jest-axe assertions in
 * __tests__/case-results.test.tsx.
 *
 * Exports:
 *   runAxeOnHtml(html: string): Promise<{ violations: number }>
 *     Testable helper — parses the HTML string in the current DOM
 *     environment (jsdom in vitest, real browser in CI) and counts
 *     serious+critical violations via axe-core. Used by
 *     __tests__/a11y-scan.test.ts to smoke-test this module without
 *     launching a browser.
 *
 * Usage (CI / local):
 *   node scripts/a11y-scan.mjs
 *
 *   Env vars:
 *     JWT_DEV_SECRET   — shared with api-gateway; falls back to dev placeholder.
 *     NEXT_PORT        — port for `next start` (default 3030).
 *     REPORT_PATH      — path for the JSON report (default .a11y-report.json).
 */

import { fileURLToPath } from "url";
import { writeFileSync } from "fs";
import { spawn } from "child_process";

// ---------------------------------------------------------------------------
// Testable helper — works in any DOM environment (jsdom or real browser)
// ---------------------------------------------------------------------------

/**
 * Run axe-core on an HTML string and return the count of serious/critical
 * violations. Requires `document` to be present in the current environment.
 *
 * @param {string} html   HTML fragment or full document.
 * @returns {Promise<{ violations: number }>}
 */
export async function runAxeOnHtml(html) {
  if (typeof document === "undefined") {
    throw new Error(
      "runAxeOnHtml requires a DOM environment (jsdom or browser). " +
        "In Node.js, use the Playwright scan instead."
    );
  }

  // Dynamic import keeps playwright (used only in main()) out of the jsdom
  // test bundle — axe-core alone is the jsdom-compatible dependency here.
  const { default: axeCore } = await import("axe-core");

  const container = document.createElement("div");
  container.innerHTML = html;
  document.body.appendChild(container);

  try {
    const results = await axeCore.run(container, {
      resultTypes: ["violations"],
    });
    const serious = results.violations.filter(
      (v) => v.impact === "serious" || v.impact === "critical"
    );
    return { violations: serious.length };
  } finally {
    document.body.removeChild(container);
  }
}

// ---------------------------------------------------------------------------
// Main scan — only runs when this script is the entry point
// ---------------------------------------------------------------------------

const isMain =
  process.argv[1] !== undefined &&
  fileURLToPath(import.meta.url) === process.argv[1];

if (isMain) {
  await main();
}

async function main() {
  const PORT = process.env.NEXT_PORT ?? "3030";
  const BASE_URL = `http://localhost:${PORT}`;
  const REPORT_PATH =
    process.env.REPORT_PATH ?? ".a11y-report.json";
  const ROUTES = ["/login", "/case/new"];
  // Impact levels that FAIL the gate.
  const BLOCKING_IMPACTS = new Set(["serious", "critical"]);

  let serverProcess = null;

  try {
    // -----------------------------------------------------------------------
    // 1. Start Next.js production server
    // -----------------------------------------------------------------------
    console.log(`[a11y-scan] Starting Next.js on port ${PORT}…`);
    serverProcess = spawn(
      "npx",
      ["next", "start", "-p", PORT],
      {
        stdio: ["ignore", "pipe", "pipe"],
        env: { ...process.env },
        shell: true,
      }
    );
    serverProcess.stdout.on("data", (d) =>
      process.stdout.write(`[next] ${d}`)
    );
    serverProcess.stderr.on("data", (d) =>
      process.stderr.write(`[next] ${d}`)
    );

    // -----------------------------------------------------------------------
    // 2. Wait for the server to accept requests (poll /login, max 60 s)
    // -----------------------------------------------------------------------
    await waitForUrl(`${BASE_URL}/login`, 60_000);
    console.log("[a11y-scan] Server ready.");

    // -----------------------------------------------------------------------
    // 3. Mint a dev session cookie by calling the login API route
    // -----------------------------------------------------------------------
    const sessionCookie = await mintDevCookie(BASE_URL);
    console.log("[a11y-scan] Session cookie minted.");

    // -----------------------------------------------------------------------
    // 4. Scan each route with Playwright + @axe-core/playwright
    // -----------------------------------------------------------------------
    const { chromium } = await import("playwright");
    const AxeBuilder = (await import("@axe-core/playwright")).default;

    const browser = await chromium.launch({ headless: true });
    const report = { scannedAt: new Date().toISOString(), pages: [] };
    let totalBlockingViolations = 0;

    for (const route of ROUTES) {
      const url = `${BASE_URL}${route}`;
      console.log(`[a11y-scan] Scanning ${url}…`);

      // /case/new is protected by middleware — pass the session cookie.
      const context = await browser.newContext();
      if (sessionCookie) {
        await context.addCookies([
          {
            name: "jp_session",
            value: sessionCookie,
            domain: "localhost",
            path: "/",
            httpOnly: true,
            sameSite: "Lax",
          },
        ]);
      }

      const page = await context.newPage();
      await page.goto(url, { waitUntil: "networkidle" });

      const results = await new AxeBuilder({ page })
        .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
        .analyze();

      const blocking = results.violations.filter((v) =>
        BLOCKING_IMPACTS.has(v.impact)
      );
      const all = results.violations;

      console.log(
        `[a11y-scan]   violations: ${all.length} total, ${blocking.length} blocking (serious/critical)`
      );

      report.pages.push({
        route,
        url,
        totalViolations: all.length,
        blockingViolations: blocking.length,
        violations: all,
      });

      totalBlockingViolations += blocking.length;
      await context.close();
    }

    await browser.close();

    // -----------------------------------------------------------------------
    // 5. Write JSON report
    // -----------------------------------------------------------------------
    writeFileSync(REPORT_PATH, JSON.stringify(report, null, 2));
    console.log(`[a11y-scan] Report written to ${REPORT_PATH}`);

    // -----------------------------------------------------------------------
    // 6. Exit non-zero if any blocking violations were found
    // -----------------------------------------------------------------------
    if (totalBlockingViolations > 0) {
      console.error(
        `[a11y-scan] FAIL — ${totalBlockingViolations} serious/critical violation(s) found. ` +
          `Download the artifact and open ${REPORT_PATH} to inspect selectors.`
      );
      process.exit(1);
    }

    console.log("[a11y-scan] PASS — no serious/critical violations.");
  } finally {
    if (serverProcess) {
      serverProcess.kill("SIGTERM");
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Poll a URL until it returns a non-5xx response, or timeout.
 */
async function waitForUrl(url, timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const res = await fetch(url);
      if (res.status < 500) return;
    } catch {
      // Server not up yet — keep polling.
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`[a11y-scan] Timed out waiting for ${url} after ${timeoutMs}ms`);
}

/**
 * POST to /api/auth/login and extract the jp_session cookie value from the
 * Set-Cookie header. Returns null if login fails (scan of /login still works;
 * /case/new will redirect to /login which axe scans instead).
 */
async function mintDevCookie(baseUrl) {
  try {
    const res = await fetch(`${baseUrl}/api/auth/login`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        email: "dev@example.test",
        password: "dev-pass",
      }),
      redirect: "manual",
    });

    const setCookie = res.headers.get("set-cookie") ?? "";
    // Parse: jp_session=<value>; ...
    const match = setCookie.match(/jp_session=([^;]+)/);
    return match ? match[1] : null;
  } catch (err) {
    console.warn("[a11y-scan] Could not mint dev cookie:", err.message);
    return null;
  }
}
