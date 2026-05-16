import type { NextConfig } from "next";

// Security headers applied to every response.  HSTS is intentionally short
// (60 days, no preload) until we have a stable HTTPS deployment.
// CSP allows inline styles because Tailwind + shadcn/ui inject runtime classes;
// `'unsafe-eval'` is required by Next.js's dev runtime and stripped in prod
// builds (Next sets a stricter prod CSP when `dev` is false).
const SECURITY_HEADERS: { key: string; value: string }[] = [
  { key: "Content-Security-Policy",
    value: "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; form-action 'self'; base-uri 'self'; object-src 'none'" },
  { key: "Strict-Transport-Security", value: "max-age=5184000; includeSubDomains" },
  { key: "X-Content-Type-Options", value: "nosniff" },
  { key: "X-Frame-Options", value: "DENY" },
  { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
  { key: "Permissions-Policy",
    value: "accelerometer=(), camera=(), geolocation=(), gyroscope=(), magnetometer=(), microphone=(), payment=(), usb=()" },
];

const nextConfig: NextConfig = {
  // Required for the docker runner stage to ship a minimal node_modules + server.js.
  output: "standalone",
  // @react-pdf/renderer uses Node.js internals (Buffer, streams) that cannot
  // be bundled by webpack/turbopack.  Marking it external keeps it as a
  // server-only require() so route handlers and Server Components work correctly.
  serverExternalPackages: ["@react-pdf/renderer"],
  // Strip the X-Powered-By: Next.js banner (PEN audit / A05 — minor info leak).
  poweredByHeader: false,
  async headers() {
    return [{ source: "/:path*", headers: SECURITY_HEADERS }];
  },
};

export default nextConfig;
