# =============================================================================
# web — Next.js 15 standalone build
# Stage 1: deps   — npm install (cached layer)
# Stage 2: build  — next build with output: "standalone"
# Stage 3: runner — minimal node:22-alpine with the standalone bundle
# =============================================================================

# ---- deps -------------------------------------------------------------------
FROM node:22-alpine AS deps
WORKDIR /build

# package.json + lockfile only, for layer-cache friendliness.
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund --prefer-offline

# ---- build ------------------------------------------------------------------
FROM node:22-alpine AS build
WORKDIR /build

COPY --from=deps /build/node_modules ./node_modules
COPY . .

# Build-time env: must be set or build fails on missing values.
ENV NEXT_TELEMETRY_DISABLED=1
ENV NODE_ENV=production

RUN npm run build

# ---- runner -----------------------------------------------------------------
FROM node:22-alpine AS runner
WORKDIR /app

RUN addgroup -g 10001 -S app && adduser -u 10001 -S -G app app

ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1
ENV PORT=3030
ENV HOSTNAME=0.0.0.0

# Standalone output bundles a minimal node_modules + server.js
COPY --from=build --chown=app:app /build/.next/standalone ./
COPY --from=build --chown=app:app /build/.next/static ./.next/static
COPY --from=build --chown=app:app /build/public ./public

USER app
EXPOSE 3030

CMD ["node", "server.js"]
