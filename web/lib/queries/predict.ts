import { gql } from "@apollo/client";
import type { CaseType } from "@/lib/case-types";
import type { Jurisdiction } from "@/lib/jurisdictions";

// ---------------------------------------------------------------------------
// GraphQL mutations
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

/** S4.4: create + persist a case, returning the full Case with server UUID. */
export const CREATE_CASE = gql`
  mutation CreateCase($input: PredictInput!) {
    createCase(input: $input) {
      id
      tenantId
      inputFeatures
      prediction {
        pWin
        ciLower
        ciUpper
        coverage
        modelVersion
        predictedAtUnix
      }
      recommendation {
        kind
        rationaleBullets
        expectedValueTry
        expectedValueSettle
      }
      createdBy
      createdAt
    }
  }
`;

// ---------------------------------------------------------------------------
// GraphQL query
// ---------------------------------------------------------------------------

/** S4.4: load a single persisted case by server UUID, scoped to the caller's tenant. */
export const GET_CASE = gql`
  query GetCase($id: ID!) {
    case(id: $id) {
      id
      tenantId
      inputFeatures
      prediction {
        pWin
        ciLower
        ciUpper
        coverage
        modelVersion
        predictedAtUnix
      }
      recommendation {
        kind
        rationaleBullets
        expectedValueTry
        expectedValueSettle
      }
      createdBy
      createdAt
    }
  }
`;

// ---------------------------------------------------------------------------
// TypeScript types mirroring the GraphQL schema
// ---------------------------------------------------------------------------

/** Input to the predictCaseOutcome / createCase mutations (Tier-A/B features only). */
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

/** Server-computed recommendation from decision-arith. */
export interface RecommendationResult {
  /** "Try" | "Settle" | "Borderline" */
  kind: string;
  /** Three deterministic reasoning bullets. */
  rationaleBullets: string[];
  /** Expected value of trial as a decimal string (e.g. "-20000.00"). */
  expectedValueTry: string;
  /** Expected value of settlement as a decimal string (e.g. "40000.00"). */
  expectedValueSettle: string;
}

/** A persisted case returned by createCase or the case(id) query. */
export interface CaseResult {
  id: string;
  tenantId: string;
  inputFeatures: PredictInput;
  prediction: PredictResult;
  recommendation: RecommendationResult;
  createdBy: string | null;
  createdAt: string;
}

export interface PredictCaseOutcomeData {
  predictCaseOutcome: PredictResult;
}

export interface PredictCaseOutcomeVars {
  input: PredictInput;
}

export interface CreateCaseData {
  createCase: CaseResult;
}

export interface CreateCaseVars {
  input: PredictInput;
}

export interface GetCaseData {
  case: CaseResult | null;
}

export interface GetCaseVars {
  id: string;
}

// ---------------------------------------------------------------------------
// S4.5: list query (fields kept minimal — only what the table view shows)
// ---------------------------------------------------------------------------

/** S4.5: paginated list of cases for the /cases page. */
export const LIST_CASES = gql`
  query ListCases($limit: Int, $offset: Int) {
    listCases(limit: $limit, offset: $offset) {
      nodes {
        id
        inputFeatures
        prediction {
          pWin
        }
        recommendation {
          kind
        }
        createdAt
        createdBy
      }
      totalCount
      nextOffset
    }
  }
`;

/** Minimal case shape returned by the list query (no full prediction detail). */
export interface CaseSummary {
  id: string;
  inputFeatures: Pick<PredictInput, "caseType" | "jurisdiction">;
  prediction: Pick<PredictResult, "pWin">;
  recommendation: Pick<RecommendationResult, "kind">;
  createdAt: string;
  createdBy: string | null;
}

/** Paginated connection returned by listCases. */
export interface CaseConnection {
  nodes: CaseSummary[];
  totalCount: number;
  /** Present when there is a next page; null when on the last page. */
  nextOffset: number | null;
}

export interface ListCasesData {
  listCases: CaseConnection;
}

export interface ListCasesVars {
  limit?: number;
  offset?: number;
}
