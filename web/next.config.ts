import type { NextConfig } from "next";

const IS_PROD = process.env.NODE_ENV === "production";

// CSP — strict by default. Dev mode keeps `'unsafe-eval'` because Next.js's
// fast-refresh + RSC runtime needs eval(). Prod drops it.
// Tailwind/shadcn inject style classes at runtime, so `'unsafe-inline'` for
// styles stays in both modes.
const CSP_SCRIPT_SRC = IS_PROD
  ? "'self' 'unsafe-inline'"
  : "'self' 'unsafe-inline' 'unsafe-eval'";

const CSP = [
  "default-src 'self'",
  `script-src ${CSP_SCRIPT_SRC}`,
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data: blob:",
  "font-src 'self' data:",
  "connect-src 'self'",
  "frame-ancestors 'none'",
  "form-action 'self'",
  "base-uri 'self'",
  "object-src 'none'",
].join("; ");

// HSTS — only meaningful over HTTPS. Set it always (browsers ignore it on
// http://) so the header is correct as soon as we terminate TLS upstream.
const SECURITY_HEADERS: { key: string; value: string }[] = [
  { key: "Content-Security-Policy", value: CSP },
  { key: "Strict-Transport-Security", value: "max-age=15552000; includeSubDomains" },
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
