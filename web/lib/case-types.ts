/** Canonical case-type values accepted by the api-gateway predictCaseOutcome mutation. */
export const CASE_TYPES = [
  { value: "civil", label: "Civil" },
  { value: "criminal", label: "Criminal" },
  { value: "bankruptcy", label: "Bankruptcy" },
] as const;

export type CaseType = (typeof CASE_TYPES)[number]["value"];
