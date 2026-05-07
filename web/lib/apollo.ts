"use client";

import {
  ApolloClient,
  InMemoryCache,
  createHttpLink,
  from,
} from "@apollo/client";
import { setContext } from "@apollo/client/link/context";

const GATEWAY_URL =
  process.env.NEXT_PUBLIC_GRAPHQL_URL ?? "http://localhost:4000/graphql";

const httpLink = createHttpLink({
  uri: GATEWAY_URL,
});

/**
 * Auth link — attaches JWT from cookie (jp_token) or localStorage as a
 * Bearer token.  Pure parse only: validation happens on the gateway.
 */
const authLink = setContext((_, { headers }) => {
  let token: string | null = null;

  if (typeof window !== "undefined") {
    // Prefer cookie; fall back to localStorage.
    const cookieMatch = document.cookie
      .split("; ")
      .find((row) => row.startsWith("jp_token="));
    token = cookieMatch
      ? decodeURIComponent(cookieMatch.split("=")[1])
      : localStorage.getItem("jp_token");
  }

  return {
    headers: {
      ...headers,
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
  };
});

export function makeApolloClient() {
  return new ApolloClient({
    link: from([authLink, httpLink]),
    cache: new InMemoryCache(),
  });
}
