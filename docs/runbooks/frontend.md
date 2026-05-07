# Frontend Runbook — JudicialPredict Web Workspace

**Sprint:** 2 (S2.14)
**Stack:** Next.js 15 · React 19 · Tailwind 4 · shadcn/ui (New York / Slate) · Apollo Client · Storybook · Chromatic · Vitest · axe-core

---

## Quick Start

```bash
cd projects/judicialpredict/web
npm install
npm run dev          # http://localhost:3000 (Turbopack)
npm run storybook    # http://localhost:6006
npm run test         # vitest run (CI mode)
npm run lint         # eslint
npm run build        # Next.js production build (Turbopack)
```

---

## Environment Variables

| Variable | Default | Purpose |
|---|---|---|
| `NEXT_PUBLIC_GRAPHQL_URL` | `http://localhost:4000/graphql` | Apollo Client endpoint (public) |
| `GATEWAY_INTERNAL_URL` | `http://localhost:4000` | Server-side healthz fetch |

Copy `.env.example` to `.env.local` and fill in values for non-default environments.

---

## Project Structure

```
web/
├── app/
│   ├── layout.tsx          # Root layout — ApolloClientProvider + AuthProvider
│   ├── page.tsx            # / route — healthz server component
│   └── globals.css         # Tailwind 4 + shadcn CSS variables (Slate)
├── components/
│   └── ui/                 # shadcn/ui components
│       ├── button.tsx
│       ├── card.tsx
│       ├── sonner.tsx       # Toast (shadcn sonner, replaces deprecated toast)
│       └── Button.stories.tsx
├── lib/
│   ├── apollo.ts           # Apollo Client factory (httpLink + authLink)
│   ├── apollo-provider.tsx # Client-side ApolloProvider wrapper
│   ├── auth-context.tsx    # JWT parse context + useTenant() + useAuth()
│   └── utils.ts            # cn() — clsx + tailwind-merge
├── __tests__/
│   └── a11y.test.tsx       # axe-core CI gate
├── stories/                # Storybook example stories (generated)
├── .storybook/
│   ├── main.ts             # Framework: @storybook/nextjs-vite
│   └── preview.ts
├── vitest.config.ts
├── vitest.setup.ts
└── components.json         # shadcn config (New York / Slate / CSS vars)
```

---

## Apollo Client

- **Entry point:** `lib/apollo.ts` — `makeApolloClient()` returns a fresh `ApolloClient`.
- **Auth link:** reads JWT from `jp_token` cookie (preferred) or `localStorage`. Attaches as `Authorization: Bearer <token>`. Pure browser read — no verification.
- **Provider:** `lib/apollo-provider.tsx` (`"use client"`) wraps children with `<ApolloProvider>`. Mounted in `app/layout.tsx`.

---

## Auth Context

`lib/auth-context.tsx` — parses JWT claims using `jose`'s `decodeJwt` (base64 decode only, **no signature verification**).

```tsx
import { useTenant, useAuth } from "@/lib/auth-context";

// In any client component:
const tenantId = useTenant();        // "acme-law" | null
const { claims, token } = useAuth(); // full claims
```

**JpClaims shape:**

```ts
{
  sub: string;       // user id
  tenantId: string;  // firm slug
  email?: string;
  roles?: string[];
  exp?: number;
  iat?: number;
}
```

JWT validation happens on the api-gateway (Rust). The client only reads claims to drive UI state (e.g. tenant-scoped labels, role-gated nav items).

---

## shadcn/ui

Config: `components.json` — New York style, Slate base colour, CSS variables, App Router (`rsc: true`).

Add new components:

```bash
npx shadcn@latest add <component-name> --yes
```

Components live in `components/ui/`. Do not edit generated files directly — re-add with `--yes --overwrite` if the upstream registry updates.

---

## Testing

### Run tests

```bash
npm run test         # vitest run (all tests, CI mode)
npm run test:watch   # vitest watch (dev mode)
```

### axe-core CI gate

`__tests__/a11y.test.tsx` renders the / route markup and asserts zero axe violations for both the healthy and unreachable states. This test **must pass** before any PR is merged.

### Adding new a11y tests

Extend `__tests__/a11y.test.tsx` or co-locate `*.a11y.test.tsx` files next to components.

---

## Storybook

```bash
npm run storybook         # dev server — http://localhost:6006
npm run build-storybook   # static build → storybook-static/
```

Stories live in:
- `stories/` — Storybook-generated example stories (keep for reference)
- `components/ui/*.stories.tsx` — shadcn component stories

### Addon: a11y

`@storybook/addon-a11y` is installed and active. Use the **Accessibility** panel in the Storybook UI to check each story for WCAG violations during development.

---

## Chromatic Visual Testing

Chromatic provides snapshot-based regression testing against the Storybook baseline.

### Setup (one-time, requires CI secret)

1. Create a Chromatic project at https://www.chromatic.com/ linked to the GitHub repo.
2. Copy the project token.
3. Set `CHROMATIC_PROJECT_TOKEN` as a CI secret (GitHub Actions → Repository Secrets).

### Running Chromatic

```bash
# Local (requires CHROMATIC_PROJECT_TOKEN in env)
CHROMATIC_PROJECT_TOKEN=<token> npm run chromatic

# CI — add to GitHub Actions workflow:
- name: Chromatic visual tests
  uses: chromaui/action@latest
  with:
    projectToken: ${{ secrets.CHROMATIC_PROJECT_TOKEN }}
    workingDir: projects/judicialpredict/web
```

### Baseline

The initial baseline is captured on the first successful Chromatic run after the token is wired. All subsequent PRs compare against that baseline. Approve visual diffs in the Chromatic dashboard before merging.

**Status:** Chromatic dep (`@chromatic-com/storybook`) is installed and the `npm run chromatic` script is wired. Actual publish is deferred until Sprint 3 when the CI token lands.

---

## Deferred to Sprint 3+

| Feature | Notes |
|---|---|
| SSO / Auth0 integration | `AuthProvider` is wired for JWT parse; SSO flows are Sprint 3 |
| Protected routes | Middleware + redirect logic — Sprint 3 |
| Chromatic publish | Token required; CI workflow is written, token not yet provisioned |
| GraphQL codegen | Schema-driven type generation once gateway GraphQL schema is stable |
| Firm Dashboard | Case list — Sprint 3 |
| Case Intake wizard | Sprint 4+ |
