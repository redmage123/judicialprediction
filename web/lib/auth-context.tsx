"use client";

/**
 * Auth context — parses JWT claims from cookie / localStorage.
 * NO signature verification (that is the gateway's responsibility).
 * Surfaces useTenant() hook for components that need tenant-scoped data.
 */

import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { decodeJwt } from "jose";

export interface JpClaims {
  sub: string;          // user id
  tenantId: string;     // firm / tenant slug
  email?: string;
  roles?: string[];
  exp?: number;
  iat?: number;
}

interface AuthState {
  claims: JpClaims | null;
  /** Raw token string (for downstream use only — do not verify client-side). */
  token: string | null;
  isLoading: boolean;
}

const AuthContext = createContext<AuthState>({
  claims: null,
  token: null,
  isLoading: true,
});

function readTokenFromBrowser(): string | null {
  if (typeof window === "undefined") return null;
  const cookieMatch = document.cookie
    .split("; ")
    .find((row) => row.startsWith("jp_token="));
  if (cookieMatch) return decodeURIComponent(cookieMatch.split("=")[1]);
  return localStorage.getItem("jp_token");
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AuthState>({
    claims: null,
    token: null,
    isLoading: true,
  });

  useEffect(() => {
    const token = readTokenFromBrowser();
    if (!token) {
      setState({ claims: null, token: null, isLoading: false });
      return;
    }

    try {
      // decodeJwt does base64 decode only — no signature check.
      const payload = decodeJwt(token) as JpClaims;
      setState({ claims: payload, token, isLoading: false });
    } catch {
      // Malformed token — treat as unauthenticated.
      setState({ claims: null, token: null, isLoading: false });
    }
  }, []);

  return <AuthContext.Provider value={state}>{children}</AuthContext.Provider>;
}

/** Returns the current tenant slug (or null when unauthenticated). */
export function useTenant(): string | null {
  const { claims } = useContext(AuthContext);
  return claims?.tenantId ?? null;
}

/** Full auth state for components that need more than just the tenant. */
export function useAuth(): AuthState {
  return useContext(AuthContext);
}
