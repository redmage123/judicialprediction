import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // @react-pdf/renderer uses Node.js internals (Buffer, streams) that cannot
  // be bundled by webpack/turbopack.  Marking it external keeps it as a
  // server-only require() so route handlers and Server Components work correctly.
  serverExternalPackages: ["@react-pdf/renderer"],
};

export default nextConfig;
