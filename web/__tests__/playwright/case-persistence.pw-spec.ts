/**
 * E2E Playwright test — case persistence (S4.4 / JP-58).
 *
 * Verifies that:
 *  1. Login as dev-tenant1 succeeds.
 *  2. Submitting a case at /case/new redirects to /case/<server-uuid>.
 *  3. Reloading the page still renders the case.
 *  4. Navigating to /case/<server-uuid> directly in a new tab still renders the case.
 *
 * STATUS: .skip — Playwright is configured in package.json but the
 * playwright.config.ts wiring to start api-gateway + Next.js together is
 * deferred to Sprint-4 wave-3. The test file is present so the spec is
 * discoverable and CI can run it once infra wiring lands.
 *
 * Sprint-4 wave-3 follow-up:
 *  - Add playwright.config.ts with webServer entries for both api-gateway and
 *    the Next.js dev server.
 *  - Remove `.skip` annotation below.
 *  - Add the test to the `web-a11y.yml` workflow's e2e step.
 */

import { test, expect } from "@playwright/test";

const DEV_EMAIL = "dev@example.test";
const DEV_PASSWORD = "dev-pass";

// All tests in this file are skipped until the full e2e infra is wired.
test.describe.skip("case-persistence e2e", () => {
  test("login → submit case → redirect to /case/<server-uuid>", async ({
    page,
  }) => {
    // 1. Login
    await page.goto("/login");
    await page.getByLabel(/email/i).fill(DEV_EMAIL);
    await page.getByLabel(/password/i).fill(DEV_PASSWORD);
    await page.getByRole("button", { name: /sign in/i }).click();
    await page.waitForURL("/");

    // 2. Navigate to /case/new and submit with sample data
    await page.goto("/case/new");
    await page.getByLabel(/judge severity/i).fill("0.65");
    await page.getByLabel(/attorney win rate/i).fill("0.72");
    await page.getByLabel(/ideology distance/i).fill("0.41");
    await page.getByLabel(/materiality score/i).fill("0.88");
    await page.getByLabel(/procedural motions filed/i).fill("3");
    await page.getByRole("button", { name: /run prediction/i }).click();

    // 3. Expect redirect to /case/<server-uuid>
    await page.waitForURL(/\/case\/[0-9a-f-]{36}/);
    const caseUrl = page.url();
    expect(caseUrl).toMatch(/\/case\/[0-9a-f-]{36}$/);

    // 4. The results page should render (P(win) visible)
    await expect(page.getByText(/%$/)).toBeVisible();

    // 5. Reload — server-side fetch means the case persists
    await page.reload();
    await expect(page.getByText(/%$/)).toBeVisible();

    // 6. Direct navigation in a new tab
    const caseId = caseUrl.split("/case/")[1];
    const newPage = await page.context().newPage();
    // The new tab won't have the session cookie; log in again first.
    await newPage.goto("/login");
    await newPage.getByLabel(/email/i).fill(DEV_EMAIL);
    await newPage.getByLabel(/password/i).fill(DEV_PASSWORD);
    await newPage.getByRole("button", { name: /sign in/i }).click();
    await newPage.goto(`/case/${caseId}`);
    await expect(newPage.getByText(/%$/)).toBeVisible();
    await newPage.close();
  });
});
