## Nexus frontend — multi-stage build producing a minimal standalone image.
##
## The Next.js "standalone" output packages only the files the server needs
## (excluding dev deps + build cache), cutting the image from ~800 MB → ~180 MB.

FROM node:20-alpine AS deps
WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm ci --no-audit --no-fund

FROM node:20-alpine AS builder
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .
# Disable telemetry so builds don't phone home.
ENV NEXT_TELEMETRY_DISABLED=1
RUN npm run build

FROM node:20-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1

# Run as a non-root user to reduce blast radius if the container is compromised.
RUN addgroup --system --gid 1001 nodejs \
 && adduser  --system --uid 1001 nextjs

# Copy the standalone server (which includes a minimal node_modules) + static
# assets. `.next/standalone` has a `server.js` entrypoint pre-wired.
COPY --from=builder /app/public ./public
COPY --from=builder --chown=nextjs:nodejs /app/.next/standalone ./
COPY --from=builder --chown=nextjs:nodejs /app/.next/static ./.next/static

USER nextjs
ENV PORT=3001
EXPOSE 3001
# `server.js` is emitted by Next.js into the standalone root.
CMD ["node", "server.js"]
