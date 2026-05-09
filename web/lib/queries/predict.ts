import { gql } from "@apollo/client";
import type { CaseType } from "@/lib/case-types";
import type { Jurisdiction } from "@/lib/jurisdictions";

// ---------------------------------------------------------------------------
// GraphQL mutation
// ---------------------------------------------------------------------------

export const PREDICT_CASE_OUTCOME = gql`
  mutation PredictCaseOutcome($input: PredictInput!) {
    predictCaseOutcome(input: $input) {
      pWin
      ciLower
      ciUpper
      coverage
      modelVersion
      predictedAtUnix
    }
  }
`;

// ---------------------------------------------------------------------------
// TypeScript types mirroring the GraphQL schema
// ---------------------------------------------------------------------------

/** Input to the predictCaseOutcome mutation (Tier-A/B features only). */
export interface PredictInput {
  /** Severity score for the assigned judge [0, 1]. */
  judgeSeverity: number;
  /** Historical win rate for the plaintiff attorney [0, 1]. */
  attorneyWinRate: number;
  /** Ideological distance between judge and case parties [0, 1]. */
  ideologyDistance: number;
  /** Materiality score of key evidence [0, 1]. */
  materialityScore: number;
  /** Number of procedural motions filed [0, 50]. */
  proceduralMotionCount: number;
  /** Type of case. */
  caseType: CaseType;
  /** Jurisdiction identifier. */
  jurisdiction: Jurisdiction;
}

/** Result returned by the predictCaseOutcome mutation. */
export interface PredictResult {
  /** Win probability [0, 1]. */
  pWin: number;
  /** Lower bound of 95 % confidence interval. */
  ciLower: number;
  /** Upper bound of 95 % confidence interval. */
  ciUpper: number;
  /** Fraction of training data covered by this prediction. */
  coverage: number;
  /** Model version string, e.g. "tier-ab-v1.2". */
  modelVersion: string;
  /** UNIX timestamp when the prediction was computed. */
  predictedAtUnix: number;
}

export interface PredictCaseOutcomeData {
  predictCaseOutcome: PredictResult;
}

export interface PredictCaseOutcomeVars {
  input: PredictInput;
}
