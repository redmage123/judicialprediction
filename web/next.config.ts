import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Required for the docker runner stage to ship a minimal node_modules + server.js.
  output: "standalone",
  // @react-pdf/renderer uses Node.js internals (Buffer, streams) that cannot
  // be bundled by webpack/turbopack.  Marking it external keeps it as a
  // server-only require() so route handlers and Server Components work correctly.
  serverExternalPackages: ["@react-pdf/renderer"],
};

export default nextConfig;
