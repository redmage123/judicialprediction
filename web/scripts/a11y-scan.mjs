#!/usr/bin/env node
/**
 * a11y-scan.mjs — End-to-end accessibility CI gate (S3.13, widened in S6.10).
 *
 * Runs THREE complementary scanners against the production build:
 *   - axe-core    (via @axe-core/playwright) — rule violations by impact
 *   - Pa11y       (HTML_CodeSniffer / WCAG2AA runner) — error-level issues
 *   - Lighthouse  (accessibility category) — aggregate 0..1 score
 *
 * Routes scanned:   /login, /case/new, /cases
 * Gate (S6.10 — "widening"):
 *   - axe:        FAIL on any violation at impact moderate | serious | critical
 *                 (Sprint 4 only blocked serious/critical; moderate now blocks)
 *   - pa11y:      FAIL on any issue of type "error"
 *   - lighthouse: FAIL when the accessibility score drops below 0.90
 *
 * /case/new and /cases are middleware-protected — the scan mints a dev
 * session cookie via /api/auth/login and passes it to every scanner.
 * Without a gateway, /cases renders its empty-state table (still scannable).
 *
 * Exports:
 *   runAxeOnHtml(html: string): Promise<{ violations: number }>
 *     Testable helper — parses the HTML string in the current DOM
 *     environment (jsdom in vitest, real browser in CI) and counts
 *     moderate+ violations via axe-core. Used by
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
 *     A11Y_TOOLS       — comma list to restrict scanners (default "axe,pa11y,lighthouse").
 */

import { fileURLToPath } from "url";
import { writeFileSync } from "fs";
import { spawn } from "child_process";

// ---------------------------------------------------------------------------
// Shared configuration
// ---------------------------------------------------------------------------

/** axe impact levels that FAIL the gate (S6.10: moderate added). */
const BLOCKING_IMPACTS = new Set(["moderate", "serious", "critical"]);
/** Routes covered by every scanner. */
const ROUTES = ["/login", "/case/new", "/cases"];
/** Lighthouse accessibility score below this fails the gate. */
const LIGHTHOUSE_MIN_SCORE = 0.9;

// ---------------------------------------------------------------------------
// Testable helper — works in any DOM environment (jsdom or real browser)
// ---------------------------------------------------------------------------

/**
 * Run axe-core on an HTML string and return the count of moderate+
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
    const blocking = results.violations.filter((v) =>
      BLOCKING_IMPACTS.has(v.impact)
    );
    return { violations: blocking.length };
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
  const REPORT_PATH = process.env.REPORT_PATH ?? ".a11y-report.json";
  const enabledTools = new Set(
    (process.env.A11Y_TOOLS ?? "axe,pa11y,lighthouse")
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean)
  );

  let serverProcess = null;

  try {
    // -----------------------------------------------------------------------
    // 1. Start Next.js production server (one server, all three scanners)
    // -----------------------------------------------------------------------
    console.log(`[a11y-scan] Starting Next.js on port ${PORT}…`);
    serverProcess = spawn("npx", ["next", "start", "-p", PORT], {
      stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env },
      shell: true,
    });
    serverProcess.stdout.on("data", (d) => process.stdout.write(`[next] ${d}`));
    serverProcess.stderr.on("data", (d) => process.stderr.write(`[next] ${d}`));

    // -----------------------------------------------------------------------
    // 2. Wait for the server to accept requests (poll /login, max 60 s)
    // -----------------------------------------------------------------------
    await waitForUrl(`${BASE_URL}/login`, 60_000);
    console.log("[a11y-scan] Server ready.");

    // -----------------------------------------------------------------------
    // 3. Mint a dev session cookie by calling the login API route
    // -----------------------------------------------------------------------
    const sessionCookie = await mintDevCookie(BASE_URL);
    const cookieHeader = sessionCookie ? `jp_session=${sessionCookie}` : null;
    console.log(
      sessionCookie
        ? "[a11y-scan] Session cookie minted."
        : "[a11y-scan] No session cookie — protected routes will redirect to /login."
    );

    // -----------------------------------------------------------------------
    // 4. Run each enabled scanner against every route
    // -----------------------------------------------------------------------
    const report = { scannedAt: new Date().toISOString(), routes: ROUTES, tools: {} };
    const failures = [];

    if (enabledTools.has("axe")) {
      const axe = await runAxeScan(BASE_URL, sessionCookie);
      report.tools.axe = axe;
      if (axe.failed) failures.push(`axe (${axe.totalBlocking} moderate+ violation(s))`);
    }
    if (enabledTools.has("pa11y")) {
      const pa11y = await runPa11yScan(BASE_URL, cookieHeader);
      report.tools.pa11y = pa11y;
      if (pa11y.failed) failures.push(`pa11y (${pa11y.totalErrors} error-level issue(s))`);
    }
    if (enabledTools.has("lighthouse")) {
      const lighthouse = await runLighthouseScan(BASE_URL, cookieHeader);
      report.tools.lighthouse = lighthouse;
      if (lighthouse.failed) {
        failures.push(`lighthouse (a11y score below ${LIGHTHOUSE_MIN_SCORE})`);
      }
    }

    // -----------------------------------------------------------------------
    // 5. Write JSON report
    // -----------------------------------------------------------------------
    report.ok = failures.length === 0;
    writeFileSync(REPORT_PATH, JSON.stringify(report, null, 2));
    console.log(`[a11y-scan] Report written to ${REPORT_PATH}`);

    // -----------------------------------------------------------------------
    // 6. Exit non-zero if any scanner failed
    // -----------------------------------------------------------------------
    if (failures.length > 0) {
      console.error(
        `[a11y-scan] FAIL — ${failures.join("; ")}. ` +
          `Download the artifact and open ${REPORT_PATH} to inspect selectors.`
      );
      process.exit(1);
    }
    console.log("[a11y-scan] PASS — axe / pa11y / lighthouse all within threshold.");
  } finally {
    if (serverProcess) {
      serverProcess.kill("SIGTERM");
    }
  }
}

// ---------------------------------------------------------------------------
// Scanner: axe-core via Playwright
// ---------------------------------------------------------------------------

/**
 * Scan every route with @axe-core/playwright, counting moderate+ violations.
 * @param {string} baseUrl
 * @param {string|null} sessionCookie  raw jp_session value, or null
 */
async function runAxeScan(baseUrl, sessionCookie) {
  console.log("[a11y-scan] axe-core — scanning…");
  const { chromium } = await import("playwright");
  const AxeBuilder = (await import("@axe-core/playwright")).default;

  const browser = await chromium.launch({ headless: true });
  const pages = [];
  let totalBlocking = 0;

  try {
    for (const route of ROUTES) {
      const url = `${baseUrl}${route}`;
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
      console.log(
        `[a11y-scan]   axe ${route}: ${results.violations.length} total, ` +
          `${blocking.length} blocking (moderate+)`
      );
      pages.push({
        route,
        url,
        totalViolations: results.violations.length,
        blockingViolations: blocking.length,
        violations: results.violations,
      });
      totalBlocking += blocking.length;
      await context.close();
    }
  } finally {
    await browser.close();
  }

  return { tool: "axe", failed: totalBlocking > 0, totalBlocking, pages };
}

// ---------------------------------------------------------------------------
// Scanner: Pa11y (HTML_CodeSniffer, WCAG2AA standard)
// ---------------------------------------------------------------------------

/**
 * Scan every route with Pa11y. Pa11y's bundled puppeteer Chromium is used;
 * the dev session cookie is passed as a request header so middleware-
 * protected routes render their real page instead of redirecting.
 * @param {string} baseUrl
 * @param {string|null} cookieHeader  "jp_session=<value>", or null
 */
async function runPa11yScan(baseUrl, cookieHeader) {
  console.log("[a11y-scan] pa11y — scanning…");
  const pa11y = (await import("pa11y")).default;

  const pages = [];
  let totalErrors = 0;

  for (const route of ROUTES) {
    const url = `${baseUrl}${route}`;
    const result = await pa11y(url, {
      standard: "WCAG2AA",
      runners: ["htmlcs"],
      includeWarnings: false,
      includeNotices: false,
      timeout: 60_000,
      headers: cookieHeader ? { Cookie: cookieHeader } : {},
      // --no-sandbox is required for Chromium under most CI containers.
      chromeLaunchConfig: { args: ["--no-sandbox"] },
    });
    const errors = result.issues.filter((i) => i.type === "error");
    console.log(
      `[a11y-scan]   pa11y ${route}: ${result.issues.length} issue(s), ` +
        `${errors.length} error-level`
    );
    pages.push({
      route,
      url,
      errorCount: errors.length,
      issues: result.issues,
    });
    totalErrors += errors.length;
  }

  return { tool: "pa11y", failed: totalErrors > 0, totalErrors, pages };
}

// ---------------------------------------------------------------------------
// Scanner: Lighthouse (accessibility category only)
// ---------------------------------------------------------------------------

/**
 * Scan every route with Lighthouse's accessibility audit. Chrome is launched
 * via chrome-launcher pointed at Playwright's Chromium (already installed in
 * CI), so no extra browser download is needed for this scanner.
 * @param {string} baseUrl
 * @param {string|null} cookieHeader  "jp_session=<value>", or null
 */
async function runLighthouseScan(baseUrl, cookieHeader) {
  console.log("[a11y-scan] lighthouse — scanning…");
  const { default: lighthouse } = await import("lighthouse");
  const chromeLauncher = await import("chrome-launcher");
  const { chromium } = await import("playwright");

  const chrome = await chromeLauncher.launch({
    chromePath: chromium.executablePath(),
    chromeFlags: ["--headless=new", "--no-sandbox", "--disable-gpu"],
  });

  const pages = [];
  let failed = false;

  try {
    for (const route of ROUTES) {
      const url = `${baseUrl}${route}`;
      const runnerResult = await lighthouse(url, {
        port: chrome.port,
        onlyCategories: ["accessibility"],
        output: "json",
        logLevel: "error",
        extraHeaders: cookieHeader ? { Cookie: cookieHeader } : undefined,
      });
      const score = runnerResult.lhr.categories.accessibility.score ?? 0;
      const routeFailed = score < LIGHTHOUSE_MIN_SCORE;
      if (routeFailed) failed = true;
      console.log(
        `[a11y-scan]   lighthouse ${route}: a11y score ${score.toFixed(2)} ` +
          `(min ${LIGHTHOUSE_MIN_SCORE})${routeFailed ? " — BELOW THRESHOLD" : ""}`
      );
      // Keep the report compact: failing audits only, not the full LHR.
      const failingAudits = Object.values(runnerResult.lhr.audits)
        .filter((a) => a.score !== null && a.score < 1)
        .map((a) => ({ id: a.id, title: a.title, score: a.score }));
      pages.push({ route, url, score, failed: routeFailed, failingAudits });
    }
  } finally {
    await chrome.kill();
  }

  return { tool: "lighthouse", failed, minScore: LIGHTHOUSE_MIN_SCORE, pages };
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
 * Mint a session cookie value the middleware will accept.  Two strategies:
 *
 *   1. POST to /api/auth/login (Django BFF) — works in local dev when the
 *      Django auth service is up.
 *   2. Fallback: sign a JWT directly with $JWT_DEV_SECRET (the shared dev
 *      secret middleware/api-gateway use).  This is the CI path — without
 *      it, protected routes 302 to /login and the scanners would only ever
 *      see the login page three times.
 *
 * Returns the raw jp_session value, or null if neither strategy works.
 */
async function mintDevCookie(baseUrl) {
  // Strategy 1 — real HTTP login (dev environments).
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
    const match = setCookie.match(/jp_session=([^;]+)/);
    if (match) return match[1];
  } catch {
    // Auth service not up — fall through to JWT forging.
  }

  // Strategy 2 — forge a JWT with the dev secret.  Middleware only checks
  // decode + exp; api-gateway verifies the signature, but the scan never
  // depends on a working gateway (e.g. /cases renders its empty-state
  // table when GATEWAY_INTERNAL_URL is unreachable).
  const secret = process.env.JWT_DEV_SECRET;
  if (!secret) {
    console.warn(
      "[a11y-scan] $JWT_DEV_SECRET is unset; protected routes will redirect to /login."
    );
    return null;
  }
  const { SignJWT } = await import("jose");
  const key = new TextEncoder().encode(secret);
  const now = Math.floor(Date.now() / 1000);
  return new SignJWT({ sub: "a11y-scan", email: "dev@example.test" })
    .setProtectedHeader({ alg: "HS256" })
    .setIssuedAt(now)
    .setExpirationTime(now + 3600)
    .sign(key);
}
