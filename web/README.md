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
| `/case/new` | Server + client island | Case intake form; accepts 7 Tier-A/B feature inputs, calls `predictCaseOutcome` mutation via Apollo, and routes to `/case/<uuid>` on success |
| `/case/[id]` | Server + client island | Results view for a submitted case. Reads the prediction stashed in `sessionStorage` by the intake form, computes a settle/try/borderline recommendation via `lib/recommend.ts` (TypeScript port of `rust/decision-arith/src/recommend.rs`), and renders P(win), 90% CI, expected-value comparison, and 3 reasoning bullets. Empty state shown when session data is absent. See S3.3 / JP-44. |

### Dev credentials for manual smoke

```
Email:    dev@example.test
Password: dev-pass
```

Visit `/case/new` → middleware redirects to `/login` → submit dev creds → form is accessible.

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
