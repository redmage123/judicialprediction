"use client";

import { ApolloClient, InMemoryCache, createHttpLink } from "@apollo/client";

/**
 * All GraphQL requests go through the Next.js BFF proxy at /api/graphql.
 * The proxy reads the httpOnly jp_session cookie server-side and attaches
 * Authorization: Bearer <jwt> to the upstream api-gateway request.
 *
 * This means no JWT ever touches browser JS — only the httpOnly cookie does.
 *
 * NEXT_PUBLIC_GRAPHQL_URL can be overridden in .env.local to point at a
 * different environment's BFF (e.g. staging).  Leave blank for the default
 * same-origin /api/graphql path.
 */
const BFF_GRAPHQL_URL =
  process.env.NEXT_PUBLIC_GRAPHQL_URL ?? "/api/graphql";

const httpLink = createHttpLink({
  uri: BFF_GRAPHQL_URL,
});

export function makeApolloClient() {
  return new ApolloClient({
    link: httpLink,
    cache: new InMemoryCache(),
  });
}
