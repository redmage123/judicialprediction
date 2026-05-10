This is the JudicialPredict workspace frontend — Next.js 15 + React 19 + Tailwind 4 + shadcn/ui + Apollo Client.

## Dev Login (Sprint 3 — local only, NOT production-ready)

> **Warning:** This is a development-only authentication gate with a single
> hard-coded operator and a shared secret. Do not use in production.

### Credentials

| Field    | Value                |
|----------|----------------------|
| Email    | `dev@example.test`   |
| Password | `dev-pass`           |
| Tenant   | `00000000-0000-0000-0000-000000000001` |

### Environment Variables

Create `.env.local` in this directory:

```bash
# Must match the secret configured in api-gateway (both sides share the same value).
JWT_DEV_SECRET=dev-only-NOT-A-REAL-SECRET-1234567890abcdef

# Optional: override the api-gateway base URL (default: http://localhost:4000)
GATEWAY_INTERNAL_URL=http://localhost:4000
```

If `JWT_DEV_SECRET` is unset the app falls back to the placeholder above and
logs a console warning at boot.

### Auth Flow

1. Visit any protected route (e.g. `/case/new`) — middleware redirects to `/login?next=/case/new`.
2. Submit dev credentials.
3. Server signs a JWT (HS256, 8 h TTL) and sets an `httpOnly SameSite=Lax` cookie named `jp_session`.
4. Middleware validates the cookie on each protected-route request.
5. All GraphQL goes through the BFF proxy at `/api/graphql`, which attaches `Authorization: Bearer <jwt>` server-side.

### JWT Claim Shape

```json
{
  "sub":       "00000000-0000-0000-0000-000000000002",
  "tenant_id": "00000000-0000-0000-0000-000000000001",
  "email":     "dev@example.test",
  "iss":       "judicialpredict-web",
  "aud":       "judicialpredict-api",
  "iat":       1234567890,
  "exp":       1234596090
}
```

### Clearing the Session

`jp_session` is `httpOnly` — it cannot be cleared via `document.cookie`. Options:

- Click **Sign out** in the app nav bar (calls `POST /api/auth/logout`).
- DevTools → Application → Cookies → delete `jp_session` for `localhost`.

### Sprint 4+ Follow-up

Real SSO (SAML/OIDC), multi-tenant routing, password reset, and proper
session storage are all deferred to **Sprint 4 — Authentication hardening**.

## Pages

| Route | Type | Description |
|---|---|---|
| `/` | Server | Health-check card — calls `api-gateway /healthz` and displays status |
| `/login` | Server + client island | Dev login form (see Dev Login section above) |
| `/case/new` | Server + client island | Case intake form; accepts 7 Tier-A/B feature inputs, calls `createCase` mutation via Apollo, persists the case server-side, and routes to `/case/<server-uuid>` on success. (S4.4: replaced `predictCaseOutcome` + sessionStorage path.) |
| `/case/[id]` | **Full server component** | Results view for a persisted case. `page.tsx` fetches the case from api-gateway server-side (reads `jp_session` cookie, calls `case(id)` query), then passes it as a prop to the presentational `ResultsView`. No sessionStorage. Source of truth is the `cases` table in PostgreSQL. Empty state shown when case not found or wrong tenant. See S4.4 / JP-58. |
| `/cases` | **Full server component** | Paginated list of all cases for the operator's tenant. Reads `?offset` query param (default 0, page size 20). Calls `listCases(limit, offset)` from api-gateway server-side (same direct-fetch + `jp_session` pattern as `/case/[id]`). Renders a table with date filed, case type, jurisdiction, P(win)%, recommendation badge, and a View link to `/case/[id]`. Pagination footer shows "Showing X–Y of N" with Previous/Next controls. Empty state with CTA to `/case/new`. See S4.5 / JP-59. |

### Dev credentials for manual smoke

```
Email:    dev@example.test
Password: dev-pass
```

Visit `/case/new` → middleware redirects to `/login` → submit dev creds → form is accessible.

## Memo PDF (S4.6 / JP-60)

### Strategy

**Strategy B chosen for Sprint 4** — `@react-pdf/renderer` server-rendered PDF.

| | Strategy A (Playwright) | Strategy B (@react-pdf/renderer) |
|---|---|---|
| Deploy size delta | +~100 MB (Chromium) | +~4 MB |
| Layout source | Live /case/[id] HTML | Dedicated React-PDF tree |
| Determinism | WYSIWYG | Yoga layout engine |
| Sprint-4 fit | Poor (deploy weight) | **Chosen** |

### Entry points

| File | Purpose |
|---|---|
| `web/lib/memo/case-memo.tsx` | `CaseMemo` React-PDF component (Letter, 1 page) |
| `web/app/api/case/[id]/memo.pdf/route.tsx` | `GET /api/case/:id/memo.pdf` — renders + returns PDF |
| `web/app/case/[id]/results-view.tsx` | "Download memo (PDF)" anchor button |
| `web/__tests__/case-memo.test.tsx` | 3 unit tests (Node env, @vitest-environment node) |

### Usage

The "Download memo (PDF)" button on `/case/:id` navigates to
`/api/case/:id/memo.pdf`. The route reads the `jp_session` cookie, fetches
the case from api-gateway, renders `CaseMemo` via `pdf().toBuffer()`, and
returns the bytes with `content-type: application/pdf` and
`content-disposition: attachment`.

### Sprint-5 follow-ups

- **Strategy A alternate**: Add `?print=1` branch to `/case/[id]/page.tsx`
  with a print-styled variant; swap the API route for a Playwright headless
  pass if pixel-perfect CSS parity with the live results view is required.
- **Statutory citations**: Full legal-memo formatting with statute/case-law
  footnotes (requires Sprint-5 legal-data pipeline).
- **Multi-page memos**: Auto-paginate when reasoning bullets + statutory text
  exceed one page.
- **Operator email in footer**: Wire the user-profile query so the footer shows
  the operator's email rather than the UUID.
- **Expected damages in memo**: Display operator-supplied `expectedDamages`
  once the Sprint-5 intake form accepts it.

---

### Sprint-5 follow-ups (from S4.5)

- Client-side sort by P(win) and recommendation kind on the `/cases` table.
- Filter by date range.
- CSV export of the case list.

### Sprint-5 follow-ups (from S4.4)

- Delete `lib/recommend.ts` — the client-side TypeScript port of `rust/decision-arith` is now dead code for the results view; the server-computed recommendation is used directly.
- Accept `expectedDamages` in the `createCase` input once the cost-engine is wired (sprint §5.4).
- Widen the a11y gate from `serious/critical` → all impacts.

---

## Accessibility CI

### Gate summary

Every PR that touches `web/**` triggers the `web-a11y` GitHub Actions workflow.
The gate fails if any **serious** or **critical** WCAG 2.2 AA violation is found.
`minor` and `moderate` violations are not yet blocking (Sprint 4 will widen the threshold).

### Routes scanned

| Route | Notes |
|---|---|
| `/login` | Scanned without authentication. |
| `/case/new` | Scanned with a dev session cookie (minted via `/api/auth/login`). |
| `/case/[id]` | **Not scanned in CI** — the results view requires `sessionStorage` data set by the intake form. Covered by per-page `jest-axe` assertions in `__tests__/case-results.test.tsx`. |

### Two layers of a11y checking

| Layer | Tool | When |
|---|---|---|
| Render-time markup | `jest-axe` (via vitest, jsdom) | Every `npm run test` run. Assertions in `__tests__/*.test.tsx`. |
| Built site (real browser) | `axe-core` via Playwright | Every PR (`web-a11y` workflow, step 5). |

### Debugging a failure

1. Open the failing PR → **Actions** → `a11y` job → download the `a11y-report` artifact.
2. Open `.a11y-report.json`. Each page entry lists `violations` with:
   - `id` — the axe rule name (e.g. `color-contrast`, `image-alt`).
   - `impact` — `serious` or `critical`.
   - `nodes[].target` — CSS selector of the offending element.
3. Fix the element in the component, re-push, and the gate will re-run.

### Running locally

```bash
# Build the production bundle first
npm run build

# Install Playwright browser (once per machine)
npx playwright install chromium

# Run the scan (starts next start on port 3030 automatically)
npm run a11y:scan
# or
JWT_DEV_SECRET=dev-only-NOT-A-REAL-SECRET-1234567890abcdef node scripts/a11y-scan.mjs
```

The report is written to `web/.a11y-report.json`.

### Sprint 4 follow-ups

- Widen the gate from `serious/critical` → all impacts (including `minor`/`moderate`).
- Add Pa11y as a second axe engine for cross-checking color-contrast results.
- Add Lighthouse accessibility score to the CI summary.
- Evaluate color-contrast thresholds against the design token palette.

---

## Getting Started

First, run the development server:

```bash
npm run dev
# or
yarn dev
# or
pnpm dev
# or
bun dev
```

Open [http://localhost:3000](http://localhost:3000) with your browser to see the result.

You can start editing the page by modifying `app/page.tsx`. The page auto-updates as you edit the file.

This project uses [`next/font`](https://nextjs.org/docs/app/building-your-application/optimizing/fonts) to automatically optimize and load [Geist](https://vercel.com/font), a new font family for Vercel.

## Learn More

To learn more about Next.js, take a look at the following resources:

- [Next.js Documentation](https://nextjs.org/docs) - learn about Next.js features and API.
- [Learn Next.js](https://nextjs.org/learn) - an interactive Next.js tutorial.

You can check out [the Next.js GitHub repository](https://github.com/vercel/next.js) - your feedback and contributions are welcome!

## Deploy on Vercel

The easiest way to deploy your Next.js app is to use the [Vercel Platform](https://vercel.com/new?utm_medium=default-template&filter=next.js&utm_source=create-next-app&utm_campaign=create-next-app-readme) from the creators of Next.js.

Check out our [Next.js deployment documentation](https://nextjs.org/docs/app/building-your-application/deploying) for more details.
