/** Canonical jurisdiction values accepted by the api-gateway predictCaseOutcome mutation. */
export const JURISDICTIONS = [
  { value: "us-federal", label: "US Federal" },
  { value: "ca-state", label: "California State" },
  { value: "nj-state", label: "New Jersey State" },
] as const;

export type Jurisdiction = (typeof JURISDICTIONS)[number]["value"];
