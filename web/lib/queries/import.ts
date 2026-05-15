import { gql } from "@apollo/client";
import type { CaseType } from "@/lib/case-types";
import type { Jurisdiction } from "@/lib/jurisdictions";

/**
 * S6.14 — bulk-import mutation.  Up to 50 rows per request; the gateway
 * runs the same ML + decision-arith + INSERT pipeline as `createCase` for
 * every row and returns one `ImportRowResult` per submitted row.
 */
export const IMPORT_CASES = gql`
  mutation ImportCases($rows: [ImportCaseRowInput!]!) {
    importCases(rows: $rows) {
      total
      succeeded
      failed
      results {
        rowIndex
        ok
        caseId
        error
      }
    }
  }
`;

/** Required CSV column headers (snake_case to match the GraphQL InputObject). */
export const IMPORT_CSV_HEADERS = [
  "judge_severity",
  "attorney_win_rate",
  "ideology_distance",
  "materiality_score",
  "procedural_motion_count",
  "case_type",
  "jurisdiction",
] as const;

/** Optional CSV column header.  Mirrors `createCase`'s `opinionText` arg. */
export const IMPORT_CSV_OPTIONAL_HEADERS = ["opinion_text"] as const;

/** Max rows per single sync request — see `case_import::MAX_IMPORT_ROWS`. */
export const MAX_IMPORT_ROWS = 50;

/** GraphQL ImportCaseRowInput shape — keep in lockstep with the gateway's
 *  `ImportCaseRow` (rust/api-gateway/src/case_import.rs). */
export interface ImportCaseRowInput {
  judgeSeverity: number;
  attorneyWinRate: number;
  ideologyDistance: number;
  materialityScore: number;
  proceduralMotionCount: number;
  caseType: CaseType;
  jurisdiction: Jurisdiction;
  opinionText?: string;
}

export interface ImportRowResult {
  rowIndex: number;
  ok: boolean;
  caseId: string | null;
  error: string | null;
}

export interface ImportCasesResult {
  total: number;
  succeeded: number;
  failed: number;
  results: ImportRowResult[];
}

export interface ImportCasesData {
  importCases: ImportCasesResult;
}

export interface ImportCasesVars {
  rows: ImportCaseRowInput[];
}
