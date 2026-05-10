/**
 * CaseMemo — React-PDF one-page evaluation memo for a JudicialPredict case.
 *
 * STRATEGY: Strategy B — @react-pdf/renderer server-rendered PDF (chosen for Sprint 4).
 *
 * WHY Strategy B over Strategy A (Playwright headless printing /case/[id]?print=1):
 *  - No Chromium in the production image (~0 MB vs ~100 MB deploy delta).
 *  - Deterministic, reproducible layout: ideal for a senior-partner one-pager
 *    where content precision matters more than pixel-perfect CSS parity.
 *  - No browser process lifecycle to manage or crash under load.
 *
 * Strategy A follow-up (Sprint-5):
 *  - Add a ?print=1 branch to /case/[id] page.tsx with a print-styled variant.
 *  - Route /api/case/[id]/memo.pdf to a Playwright GET handler that navigates
 *    to ${SELF_URL}/case/${id}?print=1 with the operator's session cookie and
 *    calls page.pdf() for pixel-perfect parity with the results view.
 */

import React from "react";
import { Document, Page, View, Text, StyleSheet } from "@react-pdf/renderer";
import type { CaseResult } from "@/lib/queries/predict";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Format a [0,1] probability as a rounded percentage string. */
function fmtPct(p: number): string {
  return `${Math.round(p * 100)}%`;
}

/**
 * Format a decimal string (e.g. "70000.00") as a USD currency string.
 * Falls back to the raw string if it cannot be parsed.
 */
function fmtDollar(s: string): string {
  const n = parseFloat(s);
  if (!Number.isFinite(n)) return s;
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 0,
  }).format(n);
}

/** Convert a Unix epoch second to a human-readable UTC string. */
function unixToUtc(unix: number): string {
  return new Date(unix * 1000).toISOString().replace("T", " ").replace(".000Z", " UTC");
}

// ---------------------------------------------------------------------------
// Styles — Times-Roman 11pt body, Helvetica 14pt headers (built-in PDF fonts)
// ---------------------------------------------------------------------------

const S = StyleSheet.create({
  page: {
    paddingTop: 48,
    paddingBottom: 60,
    paddingLeft: 56,
    paddingRight: 56,
    fontFamily: "Times-Roman",
    fontSize: 11,
    color: "#111111",
    flexDirection: "column",
  },

  // ---- Header band ---------------------------------------------------------
  headerRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    marginBottom: 6,
  },
  headerTitle: {
    fontFamily: "Helvetica-Bold",
    fontSize: 14,
    color: "#111111",
  },
  headerRight: {
    fontFamily: "Times-Roman",
    fontSize: 10,
    textAlign: "right",
    color: "#555555",
  },
  divider: {
    borderBottomWidth: 1,
    borderBottomColor: "#aaaaaa",
    marginTop: 4,
    marginBottom: 14,
  },

  // ---- Case identifier strip -----------------------------------------------
  caseStrip: {
    fontFamily: "Helvetica",
    fontSize: 11,
    color: "#333333",
    marginBottom: 14,
  },

  // ---- Big metric row ------------------------------------------------------
  metricValue: {
    fontFamily: "Helvetica-Bold",
    fontSize: 18,
    marginBottom: 4,
  },
  metricSub: {
    fontFamily: "Times-Roman",
    fontSize: 11,
    color: "#555555",
    marginBottom: 16,
  },

  // ---- Shared section label ------------------------------------------------
  sectionLabel: {
    fontFamily: "Helvetica-Bold",
    fontSize: 12,
    marginBottom: 6,
  },

  // ---- Recommendation block ------------------------------------------------
  recKind: {
    fontFamily: "Helvetica-Bold",
    fontSize: 13,
    marginBottom: 8,
  },
  bullet: {
    fontFamily: "Times-Roman",
    fontSize: 11,
    marginBottom: 4,
    paddingLeft: 12,
  },
  sectionGap: {
    marginBottom: 16,
  },

  // ---- EV comparison -------------------------------------------------------
  evRow: {
    flexDirection: "row",
    marginBottom: 16,
  },
  evCellLeft: {
    flex: 1,
    backgroundColor: "#f5f5f5",
    padding: 10,
    marginRight: 12,
  },
  evCellRight: {
    flex: 1,
    backgroundColor: "#f5f5f5",
    padding: 10,
  },
  evLabel: {
    fontFamily: "Helvetica",
    fontSize: 10,
    color: "#555555",
    marginBottom: 4,
  },
  evValue: {
    fontFamily: "Helvetica-Bold",
    fontSize: 13,
  },

  // ---- Footer --------------------------------------------------------------
  footer: {
    position: "absolute",
    bottom: 40,
    left: 56,
    right: 56,
    borderTopWidth: 1,
    borderTopColor: "#aaaaaa",
    paddingTop: 6,
    flexDirection: "row",
    flexWrap: "wrap",
  },
  footerItem: {
    fontFamily: "Times-Roman",
    fontSize: 9,
    color: "#777777",
    marginRight: 20,
  },
});

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/**
 * React-PDF document for a JudicialPredict case evaluation memo.
 *
 * Renders a single Letter-sized page suitable for senior-partner review.
 * All content is derived from `caseResult` — no placeholder strings.
 *
 * Typography:
 *  - Body: Times-Roman 11pt
 *  - Headers / labels: Helvetica-Bold 12–18pt (built-in, no external fonts)
 *
 * Sprint-5 follow-ups:
 *  - Add real operator email to the footer once the user-profile query is wired.
 *  - Accept `expectedDamages` from the intake form and display it in the EV row.
 *  - Multi-page memo for complex cases with lengthy reasoning or statutory citations.
 *
 * @param caseResult - Full persisted case as returned by the api-gateway
 *   `case(id)` GraphQL query (same shape used by the /case/[id] RSC page).
 */
export default function CaseMemo({
  caseResult,
}: {
  caseResult: CaseResult;
}): React.ReactElement {
  const {
    id,
    tenantId,
    inputFeatures,
    prediction,
    recommendation,
    createdBy,
    createdAt,
  } = caseResult;

  return (
    <Document>
      <Page size="LETTER" style={S.page}>
        {/* ── Header band ───────────────────────────────────────────────── */}
        <View style={S.headerRow}>
          <Text style={S.headerTitle}>JudicialPredict — Case Evaluation</Text>
          <Text style={S.headerRight}>
            {tenantId}{"\n"}{createdAt}
          </Text>
        </View>
        <View style={S.divider} />

        {/* ── Case identifier strip ─────────────────────────────────────── */}
        <Text style={S.caseStrip}>
          {inputFeatures.caseType}{"  ·  "}{inputFeatures.jurisdiction}{"  ·  ID: "}{id}
        </Text>

        {/* ── Big metric row ────────────────────────────────────────────── */}
        <Text style={S.metricValue}>
          Probability of plaintiff win: {fmtPct(prediction.pWin)}
        </Text>
        <Text style={S.metricSub}>
          {"CI: ["}
          {prediction.ciLower.toFixed(2)}
          {", "}
          {prediction.ciUpper.toFixed(2)}
          {"]  ("}
          {Math.round(prediction.coverage * 100)}
          {"% conformal)"}
        </Text>

        {/* ── Recommendation block ──────────────────────────────────────── */}
        <Text style={S.sectionLabel}>Recommendation</Text>
        <Text style={S.recKind}>
          {"RECOMMENDATION: "}
          {recommendation.kind.toUpperCase()}
        </Text>
        {recommendation.rationaleBullets.map((bullet, i) => (
          <Text key={i} style={S.bullet}>
            {"• "}{bullet}
          </Text>
        ))}
        <View style={S.sectionGap} />

        {/* ── Expected value comparison ─────────────────────────────────── */}
        <Text style={S.sectionLabel}>Expected Value Comparison</Text>
        <View style={S.evRow}>
          <View style={S.evCellLeft}>
            <Text style={S.evLabel}>EXPECTED VALUE AT TRIAL</Text>
            <Text style={S.evValue}>{fmtDollar(recommendation.expectedValueTry)}</Text>
          </View>
          <View style={S.evCellRight}>
            <Text style={S.evLabel}>EXPECTED VALUE AT SETTLEMENT</Text>
            <Text style={S.evValue}>{fmtDollar(recommendation.expectedValueSettle)}</Text>
          </View>
        </View>

        {/* ── Footer (absolute-positioned to page bottom) ───────────────── */}
        <View style={S.footer}>
          <Text style={S.footerItem}>Model: {prediction.modelVersion}</Text>
          <Text style={S.footerItem}>
            Predicted: {unixToUtc(prediction.predictedAtUnix)}
          </Text>
          <Text style={S.footerItem}>Tenant: {tenantId}</Text>
          {createdBy != null && (
            <Text style={S.footerItem}>Operator: {createdBy}</Text>
          )}
        </View>
      </Page>
    </Document>
  );
}
