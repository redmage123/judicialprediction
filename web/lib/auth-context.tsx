"use client";

/**
 * Auth context — surfaces decoded JWT claims to client components.
 *
 * The jp_session cookie is httpOnly so browser JS cannot read it directly.
 * On mount the AuthProvider calls GET /api/auth/me (a server-side BFF
 * endpoint) which reads the cookie, decodes the JWT, and returns the plain
 * claims object.  NO signature verification happens client-side — that is
 * always the gateway's responsibility.
 */

import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";

/** JWT claim shape minted by /api/auth/login and verified by api-gateway. */
export interface JpClaims {
  sub: string;        // operator UUID
  tenant_id: string;  // firm / tenant UUID
  email?: string;
  exp?: number;
  iat?: number;
  iss?: string;
  aud?: string | string[];
}

interface AuthState {
  claims: JpClaims | null;
  isLoading: boolean;
}

const AuthContext = createContext<AuthState>({
  claims: null,
  isLoading: true,
});

export function AuthProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AuthState>({
    claims: null,
    isLoading: true,
  });

  useEffect(() => {
    fetch("/api/auth/me")
      .then((r) => (r.ok ? r.json() : null))
      .then((data: { claims: JpClaims | null } | null) => {
        setState({ claims: data?.claims ?? null, isLoading: false });
      })
      .catch(() => {
        setState({ claims: null, isLoading: false });
      });
  }, []);

  return <AuthContext.Provider value={state}>{children}</AuthContext.Provider>;
}

/** Returns the current tenant UUID, or null when unauthenticated. */
export function useTenant(): string | null {
  const { claims } = useContext(AuthContext);
  return claims?.tenant_id ?? null;
}

/** Full auth state for components that need more than just the tenant. */
export function useAuth(): AuthState {
  return useContext(AuthContext);
}
