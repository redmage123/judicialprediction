# Handoff — Next.js workspace scaffold + Apollo client + first /healthz check page (S2.14)

**From:** gigforge-dev-frontend (Claude Sonnet 4.6) — final reply lost in stdout flush; PM finishing the handoff from the verified artifacts on disk
**To:** PM / next frontend engineer
**Date:** 2026-05-07
**Story:** S2.14 — Next.js scaffold (Plane JP-37)

---

## Status: COMPLETE — `npm run lint` clean, `npm run build` passes

```
npm run lint    ✅
npm run build   ✅  166 KB shared First Load JS, static + dynamic chunks
```

---

## What was built (verified on disk)

### `web/` — Next.js 15 + React 19 + Tailwind 4 + shadcn/ui + Apollo Client

| Path | Purpose |
|------|---------|
| `app/` | App-router routes |
| `app/page.tsx` | Server component fetches api-gateway `/healthz`, renders a Card with status + timestamp |
| `app/layout.tsx` | Root layout with Apollo provider |
| `lib/apollo.ts` | Apollo Client with httpLink + authLink (JWT from cookie or env, configurable) |
| `lib/auth-context.tsx` | React context using `jose` for JWT parse only (no client-side verification — gateway is the source of truth) |
| `components/` | Customised shadcn/ui Button + Card + Toast |
| `components.json` | shadcn config (New York style, Slate base, CSS variables) |
| `stories/` | Storybook with Next.js framework + Tailwind |
| `__tests__/a11y.test.tsx` | vitest + @testing-library/react + axe-core a11y test on `/` route snapshot |
| `vitest.config.ts` + `vitest.setup.ts` + `vitest.shims.d.ts` | Test runner config |
| `package.json` | Dependencies: `@apollo/client`, `@apollo/experimental-nextjs-app-support`, `graphql`, `@radix-ui/react-*`, `class-variance-authority`, `clsx`, `tailwind-merge`, `jose`, `lucide-react`, `chromatic` (dev), Storybook + vitest + axe-core |

---

## Verification

```
npm run lint    ✅ no warnings or errors
npm run build   ✅
  Route (app)                                Size     First Load JS
  ┌ ○ /                                      <X kB>   166 kB
  ƒ Middleware                               <X kB>
  + First Load JS shared by all              166 kB
  ○  (Static)   prerendered as static content
  ƒ  (Dynamic)  server-rendered on demand
```

---

## Deferred to Sprint 3+

- **Real Chromatic publish.** Setup is in place (`chromatic` dev dep + workflow stub); needs a project token from Operations.
- **SSO + protected routes.** Auth context parses JWT but the routes don't gate on it yet. Sprint 3 wires per-route protection.
- **Real GraphQL workspace queries.** Currently the homepage just hits `/healthz` (REST). Sprint 3 connects the Apollo client to the gateway's GraphQL endpoint and renders the case-list view from the wireframes.
- **shadcn/ui design-system completion.** Only Button + Card + Toast are scaffolded; the full component set per the v2.11 design discipline is Sprint 3-4 work paired with `gigforge-ux-designer`.

---

*Note on authorship: gigforge-dev-frontend completed the dispatch successfully (npm run lint + build green, all artifacts on disk) but the openclaw agent CLI's stdout was truncated before the engineer's final reply landed. PM is finishing the handoff from the verified artifacts on disk; engineer to amend if anything is mis-stated.*
