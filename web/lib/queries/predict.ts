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

/**
 * S4.4: create + persist a case, returning the full Case with server UUID.
 * S6.8: `opinionText` is optional — when the operator used the prior-opinion
 * prefill, the raw text is forwarded so the server persists the NLP
 * suggestion next to the operator's final values.
 */
export const CREATE_CASE = gql`
  mutation CreateCase($input: PredictInput!, $opinionText: String) {
    createCase(input: $input, opinionText: $opinionText) {
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
        confidence
        counterRecommendation {
          kindAtCiLower
          kindAtCiUpper
          flipsWithinCi
          note
        }
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
        confidence
        counterRecommendation {
          kindAtCiLower
          kindAtCiUpper
          flipsWithinCi
          note
        }
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
  /** S6.4 — qualitative confidence band from CI width: "High" | "Medium" | "Low". */
  confidence: string;
  /** S6.4 — bound-evaluated recommendation; null unless confidence == "Low". */
  counterRecommendation: CounterRecommendation | null;
  /** Three deterministic reasoning bullets. */
  rationaleBullets: string[];
  /** Expected value of trial as a decimal string (e.g. "-20000.00"). */
  expectedValueTry: string;
  /** Expected value of settlement as a decimal string (e.g. "40000.00"). */
  expectedValueSettle: string;
}

/** S6.4 — recommendation as it would land at each CI bound. */
export interface CounterRecommendation {
  kindAtCiLower: string;
  kindAtCiUpper: string;
  flipsWithinCi: boolean;
  note: string;
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
  /**
   * S6.8 — optional raw opinion text.  When present, the gateway runs the
   * NLP extractor and persists its suggestion alongside the operator's
   * final values for later accuracy evaluation.
   */
  opinionText?: string;
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

// ---------------------------------------------------------------------------
// S4.7: repredictCase mutation + casePredictions query
// ---------------------------------------------------------------------------

/**
 * S4.7: Re-run prediction on an existing case with the latest ML model.
 * Returns the updated Case (new prediction, unchanged recommendation).
 */
export const REPREDICT_CASE = gql`
  mutation RepredictCase($id: ID!) {
    repredictCase(id: $id) {
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
        confidence
        counterRecommendation {
          kindAtCiLower
          kindAtCiUpper
          flipsWithinCi
          note
        }
        rationaleBullets
        expectedValueTry
        expectedValueSettle
      }
      createdBy
      createdAt
    }
  }
`;

/**
 * S4.7: Fetch the full prediction history for a case, most-recent-first.
 * No GraphQL is fired until the disclosure is expanded (skip: !open).
 */
export const GET_CASE_PREDICTIONS = gql`
  query GetCasePredictions($id: ID!) {
    casePredictions(id: $id) {
      id
      prediction {
        pWin
        ciLower
        ciUpper
        coverage
        modelVersion
        predictedAtUnix
      }
      modelVersion
      createdAt
    }
  }
`;

/** One entry in a case's prediction history. */
export interface PredictionHistoryEntry {
  id: string;
  /** Full prediction result for this run. */
  prediction: PredictResult;
  /** Denormalised model version for quick rendering without unwrapping prediction. */
  modelVersion: string;
  /** ISO-8601 UTC timestamp of this prediction run. */
  createdAt: string;
}

export interface RepredictCaseData {
  repredictCase: CaseResult;
}

export interface RepredictCaseVars {
  id: string;
}

export interface GetCasePredictionsData {
  casePredictions: PredictionHistoryEntry[];
}

export interface GetCasePredictionsVars {
  id: string;
}

// ---------------------------------------------------------------------------
// S5.8: extractFeatures query — suggest intake-form prefills from prior
// opinion text.  Only fields with non-null suggestions get prefilled;
// the operator can override any field before submitting.
// ---------------------------------------------------------------------------

export const EXTRACT_FEATURES = gql`
  query ExtractFeatures($text: String!) {
    extractFeatures(text: $text) {
      judgeSeverity
      judgeName
      judgeCasesAnalyzed
      caseTypeHint
      caseTypeSuggestion
      outcomeFor
      jurisdictionSuggestion
    }
  }
`;

export interface ExtractedFeatures {
  /** Suggested judgeSeverity [0, 1]; null when no known judge was found. */
  judgeSeverity: number | null;
  /** Name of the matched judge, for UI confirmation labelling. */
  judgeName: string | null;
  /** Sample size behind judgeSeverity (number of prior decisions). */
  judgeCasesAnalyzed: number | null;
  /** Tax-court sub-classification (e.g. innocent_spouse) — informational. */
  caseTypeHint: string;
  /** Suggested form CaseType value (civil/criminal/bankruptcy). */
  caseTypeSuggestion: string | null;
  /** Disposition from the opinion (petitioner/respondent/split). */
  outcomeFor: string | null;
  /** Suggested form jurisdiction value (e.g. us-federal). */
  jurisdictionSuggestion: string | null;
}

export interface ExtractFeaturesData {
  extractFeatures: ExtractedFeatures;
}

export interface ExtractFeaturesVars {
  text: string;
}
