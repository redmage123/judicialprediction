# JudicialPredict — Wireframes & Information Architecture

**Document:** Low-fidelity wireframes (Bx-style; ASCII-rendered, layout + hierarchy only — no visual design)
**Audience:** Product, frontend, design system, UX research
**Companion to:** `judicialpredict-v2-spec.md` v2.10
**Date:** 7 May 2026
**Status:** Draft for Design Review

> Wireframes are deliberately low-fidelity to keep the conversation about *layout*, *information hierarchy*, and *flow* — not colour, typography, or polish. Visual-design work happens after these are signed off.

---

## 1. Information Architecture

### 1.1 Customer-facing app (Next.js)

```
Login / SSO
  └─> Firm Dashboard (case list)
        ├─> [+ New Case] → Case Intake
        │     ├─ Step 1: Basics (jurisdiction, court, parties)
        │     ├─ Step 2: Claims / charges
        │     ├─ Step 3: Document upload + extraction
        │     ├─ Step 4: Element confirmation (HITL)
        │     └─> Case Workspace
        │
        ├─> [Open Case] → Case Workspace
        │     ├─ Summary tab (default)
        │     ├─ Outcome tab     (factor breakdown, conformal CI, MC sim)
        │     ├─ Strategy tab    (counterfactuals, lead-attorney, expert)
        │     ├─ Bargaining tab  (Nash, Rubinstein, ZOPA, prospect-theory)
        │     ├─ Comparables tab (top 5 + graph paths)
        │     ├─ Timeline tab    (SDP, survival, cost breakdown)
        │     ├─ Compliance tab  (tier disclosures, lineage)
        │     └─ Memo tab        (PDF preview + export)
        │
        ├─> Firm Admin
        │     ├─ Users + roles
        │     ├─ Tenant settings (feature-tier toggles)
        │     ├─ Federated learning opt-in + privacy budget
        │     ├─ Integrations (Clio, MyCase, NetDocs)
        │     ├─ Partner-API tokens
        │     └─ Disparate-impact reports
        │
        └─> Profile / Account
```

### 1.2 Internal admin app (Django)

```
Staff SSO
  └─> Platform Dashboard
        ├─> Tenant Management
        ├─> Rule Corpus Editor
        ├─> Argumentation Framework Editor
        ├─> Audit Log Browser
        ├─> Feature-Store Metadata + Lineage Explorer
        ├─> Proxy-Audit Dashboard
        ├─> Federated Learning Coordinator
        ├─> Disparate-Impact Reports
        └─> Partner-API Token Management
```

---

## 2. Login / SSO

```
+---------------------------------------------------------+
|                                                         |
|                     [JudicialPredict]                   |
|                                                         |
|     +-------------------------------------------+       |
|     |                                           |       |
|     |  Sign in to your firm                     |       |
|     |                                           |       |
|     |  [ Email                              ]   |       |
|     |  [ Password                           ]   |       |
|     |                                           |       |
|     |  [        Sign in        ]                |       |
|     |                                           |       |
|     |  ─── or ───                               |       |
|     |                                           |       |
|     |  [ Sign in with your firm SSO ]           |       |
|     |                                           |       |
|     |  Forgot password?                         |       |
|     +-------------------------------------------+       |
|                                                         |
|    Statistical estimates only. Not legal advice.        |
|                                                         |
+---------------------------------------------------------+
```

**Annotations**

- Disclaimer in footer is persistent across every screen — non-negotiable per §15 risk register.
- SSO is the primary path for enterprise tenants; email/password is the secondary path for solo / small-firm tenants.
- Forgot-password leads to email-only flow (never expose tenant-internal user enumeration).

---

## 3. Firm Dashboard (case list)

```
+-------------------------------------------------------------------------------------+
| [JP]  Cases   Reports   Admin                            [🔔]  [User ▾]              |
+-------------------------------------------------------------------------------------+
|  Cases                                                          [ + New Case ]      |
|                                                                                     |
|  [Search cases...]   Status [Active ▾]   Type [All ▾]   Owner [All ▾]   Sort [Recent ▾] |
|                                                                                     |
|  +-------------------------------------------------------------------------------+ |
|  | Case Title                | Type    | Stage      | Recommendation | Updated   | |
|  +-------------------------------------------------------------------------------+ |
|  | Acme v. BetaCorp          | Civil   | Discovery  |  Settle  ●     | 2h ago    | |
|  | (Contract — UCC §2-608)   |         |            |  $1.2-1.5M     |           | |
|  +-------------------------------------------------------------------------------+ |
|  | US v. Tomlinson           | Crim    | Pre-trial  |  Try     ●     | 1d ago    | |
|  | (18 USC §1343)            |         |            |  ↓ exposure    |           | |
|  +-------------------------------------------------------------------------------+ |
|  | In re Wexler              | Bkcy    | §523 disp. |  Borderline ●  | 3d ago    | |
|  | (Ch. 7 nondischargeable)  |         |            |  $42-78K       |           | |
|  +-------------------------------------------------------------------------------+ |
|  | Patel v. Patel            | Civil   | Intake     |  ⏳ Analyzing   | 4d ago    | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Showing 4 of 47   [< Prev]   1 2 3 4 5   [Next >]                                  |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- Recommendation column is the single most-glanced field. Three-state pill (`Settle` / `Try` / `Borderline`) with colour-blind-safe palette (filled / hollow / striped, never red-vs-green alone).
- Range underneath the pill shows the dollar (or sentencing) figure that matters most to the partner.
- `⏳ Analyzing` state appears when models haven't finished — takes ~30-90s for a fresh case; persists until complete.
- Search defaults to fuzzy across title + parties + claims; advanced filters above table.
- Empty state (no cases): clear "Start by creating your first case" CTA + brief explainer.

---

## 4. Case Intake — Step 1 (Basics)

```
+-------------------------------------------------------------------------------------+
| [JP]  ← Back to cases                                                                |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  New Case                                                                           |
|                                                                                     |
|  ●  Basics    ○  Claims    ○  Documents    ○  Confirm                              |
|                                                                                     |
|  +-------------------------------------------------------------------------------+ |
|  |                                                                               | |
|  |  Case title *                                                                 | |
|  |  [                                                                        ]   | |
|  |                                                                               | |
|  |  Jurisdiction *                                                               | |
|  |  [ Federal ▾ ]                                                                | |
|  |                                                                               | |
|  |  Court *                                                                      | |
|  |  [ S.D.N.Y.                                       ▾ ]                         | |
|  |                                                                               | |
|  |  Judge (optional, helps prediction quality)                                   | |
|  |  [ Hon. Margaret Chen                              ] [+ Look up]              | |
|  |                                                                               | |
|  |  Parties                                                                      | |
|  |  Plaintiff *  [                                       ] [+ add party]         | |
|  |  Defendant *  [                                       ]                       | |
|  |                                                                               | |
|  |  Opposing counsel                                                             | |
|  |  [                                                       ] [+ Look up]        | |
|  |                                                                               | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|                                          [ Save draft ]    [ Continue → ]          |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- Step indicator at top — four steps, current state filled, future hollow.
- "Look up" buttons trigger the KG lookup so judge / counsel features auto-populate. Falls back to free-text if not in KG.
- Required fields explicit (`*`); optional fields explicit too — trust through transparency about what improves prediction.
- "Save draft" is permanent — drafts persist; user can leave and come back.
- Tooltip on "Judge (optional, helps prediction quality)" explains *why* — informs the user's choice.

### Step 3 — Document Upload (different layout)

```
+-------------------------------------------------------------------------------------+
|  ●  Basics    ●  Claims    ●  Documents    ○  Confirm                              |
|                                                                                     |
|  Drop documents here, or [ Browse ]                                                 |
|  +---------------------------------------------------------------+                  |
|  |                                                               |                  |
|  |              ↓                                                |                  |
|  |        Drop PDF, DOCX, or TXT files                          |                  |
|  |                                                               |                  |
|  +---------------------------------------------------------------+                  |
|                                                                                     |
|  Uploaded                                                                           |
|  +---------------------------------------------------------------+                  |
|  | 📄 Complaint.pdf                  ✓ Extracted   12 facts found |                 |
|  | 📄 Answer.pdf                     ⏳ Extracting…              |                  |
|  | 📄 Contract_2024_03_15.pdf        ✓ Tabular: 1 schedule       |                  |
|  | 📄 Deposition_Smith.docx          ⚠ 3 ambiguities — review    |                  |
|  +---------------------------------------------------------------+                  |
|                                                                                     |
|                                          [ Save draft ]    [ Continue → ]          |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- Three states per document: ✓ Extracted, ⏳ Extracting, ⚠ Needs review. Failed-extraction state distinct (✗ Failed, with retry).
- Click any doc → side-pane opens the original PDF + the extracted structured facts side-by-side.
- "Needs review" is the active-learning queue — the LLM was uncertain, the user is asked to confirm.

### Step 4 — Element Confirmation (HITL)

```
+-------------------------------------------------------------------------------------+
|  ●  Basics    ●  Claims    ●  Documents    ●  Confirm                              |
|                                                                                     |
|  Review extracted facts                                                             |
|                                                                                     |
|  Cause of action: Breach of Contract (UCC §2-608)                                   |
|                                                                                     |
|  Element 1 — Existence of contract                                                  |
|  +-------------------------------------------------------------------------------+ |
|  | Source: Complaint.pdf, ¶ 8                                                    | |
|  | "On March 15, 2024, Plaintiff and Defendant entered into a written agreement…"| |
|  | Extracted: ✓ Contract exists, executed 2024-03-15                             | |
|  | Confidence: ████████░░  82%                                                   | |
|  | [ Confirm ]   [ Edit ]   [ Mark uncertain ]                                   | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Element 2 — Material breach (fuzzy)                                                |
|  +-------------------------------------------------------------------------------+ |
|  | Source: Complaint.pdf, ¶¶ 12-18                                               | |
|  | Materiality score:  ▓▓▓▓▓▓░░░░  0.62                                          | |
|  | Drivers: dollar impact (high), time of breach (mid), deviation (high)         | |
|  | This element is on the borderline — sensitivity to materiality finding shown  | |
|  |   in the workspace. [ Adjust drivers ]                                         | |
|  | [ Confirm ]   [ Edit ]                                                        | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  [ ← Back ]                              [ Save draft ]    [ Run analysis → ]      |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- Confidence bars use bar-fill not numeric-only — easier to glance.
- Fuzzy elements explicitly distinguished from binary elements; user sees the driver decomposition.
- "Mark uncertain" flags the element for sensitivity surfacing in the workspace.
- "Run analysis" triggers all four reasoning layers + Monte Carlo simulation. Takes 30-90s; navigates to a loading workspace state.

---

## 5. Case Workspace — Summary tab (default view)

This is the most important screen in the product. The partner sees this and 80% of the time leaves having read just this.

```
+-------------------------------------------------------------------------------------+
| [JP] ← Cases    Acme v. BetaCorp · S.D.N.Y. · Hon. M. Chen     [⚙]  [User ▾]        |
+-------------------------------------------------------------------------------------+
|  Summary  Outcome  Strategy  Bargaining  Comparables  Timeline  Compliance  Memo    |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  +---------------------------------------------------------------+                  |
|  |                                                               |                  |
|  |   Recommendation:  ●  SETTLE                                  |                  |
|  |                                                               |                  |
|  |   Best settlement offer:  $1.4M    EV(trial):  $980K          |                  |
|  |   CVaR @ 5%:              −$2.1M   Settlement floor:  $1.2M   |                  |
|  |                                                               |                  |
|  |   Borderline at:  $1.05M  ←  if offer drops below this,       |                  |
|  |                              the recommendation flips to Try  |                  |
|  |                                                               |                  |
|  |   Why:  Material breach is borderline (fuzzy 0.62). Judge     |                  |
|  |   Chen's status-quo bias favors defendant on procedural        |                  |
|  |   defaults. Comparables 2/5 plaintiff wins.                   |                  |
|  |                                                               |                  |
|  +---------------------------------------------------------------+                  |
|                                                                                     |
|  P(win at trial)                              Time to resolution                    |
|  ┌─────────────────────────────────┐          Median: 14 months                     |
|  │ ████████████░░░░░░░░░░░  41%    │          90% CI:  9–24 mo.                     |
|  │ 90% CI: 33% – 49% (conformal)   │          [ See timeline → ]                    |
|  └─────────────────────────────────┘                                                |
|                                                                                     |
|  Top factors driving recommendation                                                 |
|  +-------------------------------------------------------------------------------+ |
|  | ↓ Materiality finding borderline             [ See breakdown →]               | |
|  | ↑ Judge status-quo bias (defendant-favouring)                                 | |
|  | ↓ Lead counsel record before this judge: 1-3                                  | |
|  | ↑ UCC §2-608 ruling line in 2nd Cir. lean plaintiff post-2019                | |
|  | ↓ Defendant insurance coverage caps potential exposure                       | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  [ Show full analysis ↓ ]      [ Counterfactual: what would change this? ]          |
|                                                                                     |
|  ─────────────────────────────────────────────────────────────────────────────      |
|  Tier-A judge features used (ideology, MFT, cognitive bias, HEXACO).                |
|  No Tier-C protected-class features used — cause of action does not require it.     |
|  Statistical estimates only. Not legal advice.                                      |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- **One screen, one decision.** Recommendation card is the largest element. The partner can read this card and walk into a client meeting.
- **Three numbers, not thirty.** Best offer, EV(trial), CVaR. The dollar that flips the recommendation is right next to them.
- **"Why" is one paragraph, three sentences max.** Generated by templating from top-factor outputs, not freeform LLM text.
- **Five top factors, with direction (↑/↓).** Bracketed factors expand into the Outcome tab.
- **Compliance disclosure footer** is persistent — every workspace tab shows which feature tiers were used. Tier-C usage is highlighted with statutory cite when present.
- **Disclaimer** at very bottom is non-negotiable.
- **"Show full analysis"** is the path for the associate; partner usually never clicks it.

### Loading state

```
+---------------------------------------------------------------+
|                                                               |
|  Analyzing case…                                              |
|                                                               |
|  ✓ Documents extracted (12 facts)                             |
|  ✓ Comparable cases retrieved (47 candidates → top 5)         |
|  ✓ Statutory rules applied (UCC §2-608, §2-718, §1-201)       |
|  ⏳ Running Monte Carlo simulation (4,200 / 10,000 trajectories) |
|  ⏳ Computing settlement bargaining model                      |
|  ○ Final recommendation                                       |
|                                                               |
|  Estimated time remaining: 32s                                |
|                                                               |
+---------------------------------------------------------------+
```

**Annotations**

- Process transparency is a trust signal — show the work happening, don't just spin.
- Per-step status with realistic descriptions, not generic spinners.
- ETA derived from prior runs of similar case complexity.

### Error state

```
+---------------------------------------------------------------+
|  ⚠ Couldn't complete the analysis                             |
|                                                               |
|  The Logic service is temporarily unavailable.                |
|  We've completed:                                             |
|     ✓ Document extraction                                     |
|     ✓ Comparable case retrieval                               |
|     ✓ Monte Carlo simulation                                  |
|                                                               |
|  Missing: rule-engine output, statutory analysis              |
|                                                               |
|  You can:                                                     |
|     [ Retry analysis ]   [ View partial results ]   [ Notify support ] |
|                                                               |
+---------------------------------------------------------------+
```

**Annotations**

- Honest about what failed, what completed, what's missing.
- Lets user proceed with partial results if appropriate (with very visible "Partial" banner on the workspace).

---

## 6. Case Workspace — Outcome tab

The associate's habitat. Detailed factor breakdown.

```
+-------------------------------------------------------------------------------------+
|  Summary  Outcome  Strategy  Bargaining  Comparables  Timeline  Compliance  Memo    |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  P(win at trial):  41%   90% conformal CI: 33% – 49%   Per-stratum: similar         |
|                                                                                     |
|  Factor breakdown (SHAP + GAT attention)                                            |
|  +-------------------------------------------------------------------------------+ |
|  |  Driver                                       Impact on P(win)                | |
|  |  ──────────────────────────────────────────────────────────────────────       | |
|  |  ↑ UCC §2-608 doctrinal line favours plaintiff              +0.07            | |
|  |    (2nd Cir., post-Hawkins 2019)                                              | |
|  |  ↓ Materiality finding borderline                            -0.06            | |
|  |    (fuzzy 0.62; threshold 0.70)                                               | |
|  |  ↑ Plaintiff lead counsel firm tier (AmLaw 50)               +0.04            | |
|  |  ↓ Judge Chen status-quo bias                                -0.05            | |
|  |  ↓ Lead counsel record before Chen: 1-3                      -0.03            | |
|  |  ↑ Defendant insurance fully covers exposure                 +0.02            | |
|  |  ↓ Comparable cases lean defendant 3-2                       -0.04            | |
|  |  ──────────────────────────────────────────────────────────────────────       | |
|  |  Base rate (similar cases)                                    0.46            | |
|  |  Final P(win)                                                 0.41            | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Outcome distributions                                                              |
|  +---------------------------------------------+--------------------------------+  |
|  | Damages (if plaintiff wins)                 | Verdict probability            |  |
|  |                                             |                                |  |
|  | $0 ─────────────────────────────────── $5M  | Win:    41%  ████████░░        |  |
|  |     ╱╲                                      | Loss:   55%  ██████████░       |  |
|  |    ╱  ╲                                     | Mistrial: 4% █                  |  |
|  |   ╱    ╲                                    |                                |  |
|  |  ╱      ╲___                                | (10,000 MC trajectories)       |  |
|  | 10%   median   90%                          |                                |  |
|  | $1.1M  $2.0M  $3.4M                         |                                |  |
|  +---------------------------------------------+--------------------------------+  |
|                                                                                     |
|  Sensitivity analysis                                                               |
|  +-------------------------------------------------------------------------------+ |
|  | Materiality finding:   [─────●─────────]  0.62                                | |
|  |                        not material  ←       → clearly material               | |
|  |                        At 0.70:  P(win) → 0.54                                | |
|  |                                                                               | |
|  | Lead counsel:          [ Smith (current) ▾]  → P(win) 0.41                    | |
|  |                        [ Wong            ▾]  → P(win) 0.51                    | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  ─────────────────────────────────────────────────────────────────────────────      |
|  Compliance + disclaimer footer                                                     |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- **Waterfall** for SHAP attribution — base rate at top, drivers ordered by magnitude, final P(win) at bottom. Standard chart, lawyers grok it after one explanation.
- **Two-column distribution view** — damages distribution + verdict probability side-by-side. Damages plotted with 10/50/90 quantile annotations because lawyers think in ranges.
- **Sensitivity sliders** are the killer feature. Drag the materiality slider; P(win) updates live. The "if you assigned Wong instead of Smith" pull-down is the lead-attorney optimization surfaced inline.
- **Per-stratum CI annotation** — "Per-stratum: similar" tells the user the conformal coverage is honest at this jurisdiction × case-type slice. If it weren't, this would say "wide — sparse comparables".

---

## 7. Case Workspace — Strategy tab (Counterfactuals + Lead-attorney + Expert-witness)

```
+-------------------------------------------------------------------------------------+
|  Summary  Outcome  Strategy  Bargaining  Comparables  Timeline  Compliance  Memo    |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  Counterfactual paths                                                               |
|                                                                                     |
|  Current recommendation: SETTLE                                                     |
|  Borderline at offer = $1.05M (drops below → Try)                                   |
|                                                                                     |
|  Strategic counterfactuals — what could you change going forward?                   |
|  +-------------------------------------------------------------------------------+ |
|  | ▢ Reassign lead from Smith → Wong          P(win): 0.41 → 0.51                | |
|  |   Cost impact: +$32K (Wong's hourly rate)                                     | |
|  |   Recommendation flips to: TRY at offer ≤ $1.18M                              | |
|  |                                                                               | |
|  | ▢ Add Dr. Patel as damages expert          P(win): 0.41 → 0.46                | |
|  |   Daubert risk: low (admitted in 8/9 prior 2nd Cir.)                          | |
|  |   Cost impact: +$80K expert fees                                              | |
|  |                                                                               | |
|  | ▢ File MSJ on Count II (fraud)             P(win): 0.41 → 0.43                | |
|  |   Estimated motion cost: $45K                                                 | |
|  |                                                                               | |
|  | ▢ All three above                          P(win): 0.41 → 0.58                | |
|  |   Combined cost: +$157K                                                       | |
|  |   Recommendation: TRY at any settlement offer ≤ $1.32M                        | |
|  +-------------------------------------------------------------------------------+ |
|  [ Apply selected to scenario ]                                                     |
|                                                                                     |
|  Hypothetical counterfactuals — what if a fact were found differently?              |
|  +-------------------------------------------------------------------------------+ |
|  | If material breach is found (0.70+)                P(win) 0.41 → 0.54         | |
|  | If court rejects defendant's MTD                   P(win) 0.41 → 0.46         | |
|  | If insurance coverage is exhausted                 Damages 50%-ile +$340K     | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Lead-attorney optimization                                                         |
|  +-------------------------------------------------------------------------------+ |
|  | Attorney        Past v. Chen   Personality fit  Workload   P(win)             | |
|  | ──────────────────────────────────────────────────────────────────            | |
|  | ● Smith (current)  1-3          Good            Low        0.41               | |
|  |   Wong             3-1          Excellent       Mid        0.51               | |
|  |   Liu              0-0          Good            Low        0.43               | |
|  |   Robertson        2-2          Fair            High       0.40               | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Expert-witness suggestions                                                         |
|  +-------------------------------------------------------------------------------+ |
|  | Dr. Patel (damages)        Daubert: 8/9 admitted  Cost: $80K  +0.05           | |
|  | Dr. Lin (causation)        Daubert: 5/6 admitted  Cost: $65K  +0.03           | |
|  | Dr. Foster (industry)      Daubert: 11/12 admitted Cost: $95K  +0.04          | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- **Strategic vs hypothetical counterfactuals** explicitly separated — different mental models.
- **Strategic = "things you can do."** Each shows P(win) delta + cost + recommendation flip if applicable.
- **Combined-effect row** is critical — strategic moves interact, simple addition is wrong; we show the joint counterfactual.
- **Lead-attorney table** is sortable by any column. Personality fit comes from the HEXACO + cognitive-style + Cialdini compatibility model in v2.9.
- **Expert table** includes Daubert admission record — admissibility risk surfaced upfront.

---

## 8. Case Workspace — Bargaining tab

```
+-------------------------------------------------------------------------------------+
|  Summary  Outcome  Strategy  Bargaining  Comparables  Timeline  Compliance  Memo    |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  Settlement value range                                                             |
|  +-------------------------------------------------------------------------------+ |
|  |                                                                               | |
|  |  $0          $0.5M       $1.0M       $1.5M       $2.0M       $2.5M           | |
|  |   ─────────────|───────────|───────────|───────────|───────────|              | |
|  |                                                                               | |
|  |   Defendant's WATNA          ZOPA              Plaintiff's BATNA              | |
|  |   ◆────────────────────╫─━━━━━━━━━━━╫─────────────────────◆                   | |
|  |   $620K                  $1.05M     $1.40M                  $2.1M             | |
|  |                                                                               | |
|  |   ●  Current offer: $1.4M (top of ZOPA, plaintiff-favouring)                  | |
|  |   ●  Nash bargaining solution:           $1.18M                               | |
|  |   ●  Rubinstein (patient plaintiff):     $1.27M                               | |
|  |   ●  Kalai-Smorodinsky:                  $1.22M                               | |
|  |                                                                               | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Anchor-and-adjust prediction (negotiation psychology)                              |
|  +-------------------------------------------------------------------------------+ |
|  | If you open at:    Plaintiff's likely close:        Acceptance probability    | |
|  |   $1.8M                $1.42M                          61%                    | |
|  |   $1.5M                $1.31M                          74%                    | |
|  |   $1.2M                $1.12M                          82%                    | |
|  |   $1.0M  (low anchor)  $0.94M                          71% (counter likely)   | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Procedural-justice multiplier                                                      |
|  ┌─────────────────────────────────────────────────────────────────────────────┐   |
|  │ Process fairness signal (motion grants per side, hearing time):  0.91       │   |
|  │ Settlement-acceptance lift vs pure economic value: +14%                     │   |
|  └─────────────────────────────────────────────────────────────────────────────┘   |
|                                                                                     |
|  Prospect-theory utility (your firm's loss aversion: λ = 2.25)                      |
|  +---------------------------------------------+                                    |
|  |                                              |                                    |
|  |    Utility ▲                                 |                                    |
|  |          ╱                                   |                                    |
|  |         ╱                                    |                                    |
|  |  ──────╱──────────► Outcome ($)              |                                    |
|  |        │                                     |                                    |
|  |       ╱                                      |                                    |
|  |     ╱                                        |                                    |
|  |   ╱  ← steeper here (loss aversion)          |                                    |
|  |                                              |                                    |
|  +---------------------------------------------+                                    |
|                                                                                     |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- **One-line ZOPA visualization** is the heart of this screen. Defendant's walk-away ◆ on left, plaintiff's ◆ on right, ZOPA shaded between them, current offer marker.
- **Three game-theory solutions** plotted as points on the same line — gives the partner a defensible range.
- **Anchor-adjust table** is the actionable bit — "if you open at X, here's what plaintiff likely closes at." Built from negotiation-psychology model in v2.9.
- **Procedural-justice multiplier** explicitly surfaced — settlement acceptance is influenced by perceived fairness, not just economics.
- **Prospect-theory utility curve** with the firm's specific loss-aversion parameter — explains *why* the recommendation weighs CVaR the way it does.

---

## 9. Case Workspace — Comparables tab

```
+-------------------------------------------------------------------------------------+
|  Summary  Outcome  Strategy  Bargaining  Comparables  Timeline  Compliance  Memo    |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  Top 5 comparable cases                                                             |
|                                                                                     |
|  +-------------------------------------------------------------------------------+ |
|  | 1. Hawkins v. Tritec Corp.                              Similarity: 0.91      | |
|  |    S.D.N.Y. 2022, Hon. R. Sussman                                             | |
|  |    UCC §2-608 revocation; $2.1M jury verdict for plaintiff                    | |
|  |    Why similar: same statutory subsection, similar materiality dispute,       | |
|  |    similar industry (manufacturing components), both AmLaw 50 firms           | |
|  |    [ View case →]                                                             | |
|  +-------------------------------------------------------------------------------+ |
|  | 2. Crawford Bros v. Western Steel                       Similarity: 0.84      | |
|  |    E.D.N.Y. 2021, Hon. M. Chen (same judge!)                                  | |
|  |    Settled at $890K mid-discovery                                             | |
|  |    Why similar: same judge, similar §2-608, similar plaintiff size            | |
|  +-------------------------------------------------------------------------------+ |
|  | 3. Innotech v. SilverPath                               Similarity: 0.81      | |
|  |    S.D.N.Y. 2023, Hon. P. Vargas                                              | |
|  |    Plaintiff lost on summary judgment                                         | |
|  +-------------------------------------------------------------------------------+ |
|  | 4. Bay Industries v. Coastal                            Similarity: 0.77      | |
|  |    D.N.J. 2022, Hon. S. Okafor                                                | |
|  |    Settled at $1.7M after discovery                                           | |
|  +-------------------------------------------------------------------------------+ |
|  | 5. Garrison Foods v. Apex Logistics                     Similarity: 0.74      | |
|  |    S.D.N.Y. 2020, Hon. J. Bell                                                | |
|  |    Plaintiff prevailed; $2.4M verdict                                         | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Outcome distribution across top 5                                                  |
|  +-------------------------------------------------------------------------------+ |
|  |  Plaintiff verdict  ██  2/5 (40%)                                             | |
|  |  Settlement        ██   2/5 (40%)                                             | |
|  |  Defense verdict   █    1/5 (20%)                                             | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Graph paths to comparables (interactive)                                           |
|  +-------------------------------------------------------------------------------+ |
|  |       (current case)                                                          | |
|  |          │                                                                    | |
|  |     ┌────┴─────┐                                                              | |
|  |     │          │                                                              | |
|  |  same         same statute, opposing-counsel                                  | |
|  |  judge        clique, similar damages                                          | |
|  |     │          │                                                              | |
|  |  Crawford   Hawkins ── opinion-cites ── Innotech                              | |
|  |     │                                                                          | |
|  |  Bay (different jur., similar materiality)                                    | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- **Per-comparable similarity score** with explicit "why similar" — never opaque retrieval.
- **Outcome distribution across the 5** is the most useful summary; 2/5 plaintiff means roughly 40% empirical base rate from comparables alone.
- **Graph paths** visualisation is the GAT-attention surfacing of how each comparable connects to the current case through the KG.

---

## 10. Case Workspace — Compliance tab

```
+-------------------------------------------------------------------------------------+
|  Summary  Outcome  Strategy  Bargaining  Comparables  Timeline  Compliance  Memo    |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  Feature tier disclosure                                                            |
|                                                                                     |
|  This recommendation used:                                                          |
|                                                                                     |
|  ✓ Tier-A (Judges) — full feature set                                               |
|     Ideology (Martin-Quinn, JCS, Bonica DIME)                                       |
|     HEXACO personality, MFT, cognitive-bias profile, judicial temperament           |
|     Reversal record, career path, law school                                        |
|                                                                                     |
|  ✓ Tier-B (Attorneys) — full predictive set                                         |
|     Win/loss record, personality, GDMS, Cialdini style, FEC ideology proxy          |
|                                                                                     |
|  ✓ Tier-D (Expert witnesses) — Dr. Patel, Dr. Lin                                   |
|     CV, prior testimony, Daubert record                                             |
|                                                                                     |
|  ✗ Tier-C (Parties) — none used                                                     |
|     This cause of action (UCC §2-608) does not require protected-class             |
|     status as an element. No party-level demographic features entered the          |
|     prediction.                                                                     |
|                                                                                     |
|  Federated learning                                                                 |
|  ●  Your firm's anonymised gradients did not contribute to this analysis           |
|      (this case is too recent — minimum 90 days post-resolution before              |
|       inclusion in shared model)                                                    |
|                                                                                     |
|  Privacy budget (this quarter)                                                      |
|  ────────────  ε used: 1.2 / 8.0   δ used: 1e-7 / 1e-5   [ Detail →]                |
|                                                                                     |
|  Feature lineage                                                                    |
|  [ Browse all features used → ]   [ Download as JSON ]                              |
|                                                                                     |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- **Plain-language disclosure** of which feature tiers were used and *why* the protected ones weren't. Statutory citation when Tier-C is engaged.
- **Federated learning visibility** — user can see whether their firm's data influenced the model and what their privacy budget is.
- **Lineage browse** opens a side-pane with full feature provenance for power users / auditors.

---

## 11. PDF Memo Preview

This is the artifact partners share with clients. Design quality here directly affects perceived credibility.

```
                    +---------------------------------------+
                    |                                       |
                    |        [Firm name & address]          |
                    |                                       |
                    |           CASE EVALUATION             |
                    |                                       |
                    |   Acme Industries v. BetaCorp Inc.    |
                    |   S.D.N.Y. Case No. 2024-cv-12345     |
                    |                                       |
                    |   Hon. Margaret Chen                  |
                    |   Cause of action: UCC §2-608         |
                    |                                       |
                    |   Prepared:  May 7, 2026              |
                    |   By:        [Attorney name]          |
                    |   Privileged & Confidential           |
                    |                                       |
                    |   ─────────────────────────────       |
                    |                                       |
                    |   RECOMMENDATION: SETTLE              |
                    |                                       |
                    |   Best offer:    $1.4M                |
                    |   EV(trial):     $980K                |
                    |   CVaR @ 5%:     -$2.1M               |
                    |   Borderline at: $1.05M               |
                    |                                       |
                    |   ─────────────────────────────       |
                    |                                       |
                    |   1. Executive Summary    p. 2        |
                    |   2. Outcome Forecast     p. 3        |
                    |   3. Strategic Options    p. 5        |
                    |   4. Bargaining Analysis  p. 7        |
                    |   5. Comparables          p. 9        |
                    |   6. Methodology Notes    p. 11       |
                    |   7. Compliance Statement p. 13       |
                    |                                       |
                    |   ─────────────────────────────       |
                    |   Statistical estimates only.         |
                    |   Not legal advice.                   |
                    |                                       |
                    +---------------------------------------+
                              Page 1 of 14
```

**Annotations**

- Cover page mirrors the workspace summary but in print typography.
- TOC with page references — partners flip to relevant section.
- Methodology notes (p. 11) is the explanation of the four reasoning layers in plain English; the firm can defend the recommendation methodology without re-reading the spec.
- Compliance statement (p. 13) is the printed version of the Compliance tab.
- Server-side PDF rendering (Puppeteer or WeasyPrint) over a dedicated print stylesheet — not "print this web page" with all the screen chrome.

---

## 12. Firm Admin — Dashboard

```
+-------------------------------------------------------------------------------------+
| [JP]  Cases   Reports   Admin                                            [User ▾]   |
+-------------------------------------------------------------------------------------+
|  Admin                                                                              |
|                                                                                     |
|  Users        Settings        Federation        Integrations        Reports         |
|                                                                                     |
|  Tenant: Anderson Hartley LLP                                                       |
|  Plan: Premium                                                                      |
|                                                                                     |
|  Activity (last 30 days)                                                            |
|  +-------------------------------------------------------------------------------+ |
|  |  Cases evaluated:   42        Active users:     14        Memos exported: 27  | |
|  |                                                                               | |
|  |  Recommendations breakdown:                                                   | |
|  |  Settle ████████░░░░ 24    Try ████░░░░░░░░ 11    Borderline ██░░ 7           | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Federated learning                                                                 |
|  +-------------------------------------------------------------------------------+ |
|  | Status: ● Opted in                                              [ Manage → ]  | |
|  | Privacy budget this quarter:    1.2 / 8.0 ε   |   1e-7 / 1e-5 δ                 | |
|  | Cases contributed:               18                                            | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Disparate-impact reports                                                           |
|  +-------------------------------------------------------------------------------+ |
|  | Q1 2026 report ready                                              [ View → ]  | |
|  | No flagged disparities. Tier-C usage: 3 cases (Title VII, ADA).               | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- Top-of-screen cards summarise activity; details one click away.
- Federation + privacy budget is a first-class admin concern — has a card on the dashboard.
- Disparate-impact reports surface alerts inline; "no flagged disparities" is the green-state default.

---

## 13. Django Admin — Rule Corpus Editor

```
+-------------------------------------------------------------------------------------+
| Django Admin > Rule Corpus > 11 USC §523(a)(2)(A)                                   |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  Rule:  11 USC §523(a)(2)(A) — False pretenses, false representation, actual fraud  |
|                                                                                     |
|  +--------------------------------+  +----------------------------------+           |
|  | Datalog encoding               |  | Test cases                       |           |
|  |                                |  | ──────────────────────────────── |           |
|  | nondischargeable(D, debt)      |  | ✓ Brown v. Felsen (1979)         |           |
|  |   :- false_representation(D),  |  | ✓ Field v. Mans (1995)           |           |
|  |      knowledge_of_falsity(D),  |  | ✓ Husky v. Ritz (2016)           |           |
|  |      intent_to_deceive(D),     |  | ✓ Lamar Archer (2018)            |           |
|  |      reliance_by_creditor(D),  |  | ⚠ Hypothetical: ambiguous reliance|          |
|  |      proximate_cause(D, debt). |  |   [ Review → ]                    |          |
|  |                                |  |                                   |          |
|  | [ Edit ] [ History ]           |  | [ + Add test case ]               |          |
|  +--------------------------------+  +----------------------------------+           |
|                                                                                     |
|  Source citations                                                                   |
|  +-------------------------------------------------------------------------------+ |
|  | 11 USC §523(a)(2)(A)                                                          | |
|  | Field v. Mans, 516 U.S. 59 (1995) — "actual fraud" element                     | |
|  | Husky Int'l Elecs. v. Ritz, 578 U.S. 355 (2016) — actual fraud not limited    | |
|  |   to misrepresentation                                                        | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  Versions                                                                           |
|  +-------------------------------------------------------------------------------+ |
|  |  v3 — current     2026-04-12 by [SME-MJK]   Husky update                     | |
|  |  v2               2025-11-08 by [SME-MJK]   Field v. Mans clarification       | |
|  |  v1               2025-09-22 by [SME-RBN]   Initial encoding                  | |
|  +-------------------------------------------------------------------------------+ |
|                                                                                     |
|  SME sign-off:  ✓ Approved by Mike Kessler (NY Bankruptcy SME)                      |
|                                                                                     |
+-------------------------------------------------------------------------------------+
```

**Annotations**

- **Two-column** Datalog + test cases is the heart of the editor. Edit one, run the other.
- **Property-based test gate** — saving requires test-case pass rate above threshold.
- **Source citations** are mandatory and inline. No rule without source.
- **Version history** with named author + change reason — every rule change is auditable.
- **SME sign-off** required for promotion to staging/prod environments. Django permission-class enforces it.

---

## 14. Responsive considerations

### Desktop (1440+ px) — primary

Full layout as above. All panels visible. Sensitivity sliders inline. Side-pane document review while on Outcome tab.

### Tablet landscape (1024-1279 px) — secondary

Workspace tabs collapse to dropdown. Two-column panels become one-column. Comparable-graph visualisation simplifies (text list + drill-in for graph view).

### Tablet portrait / small laptop (768-1023 px) — tertiary

Single-column throughout. Tab bar becomes horizontal scroll. Sensitivity sliders move to modal "Adjust" sheets.

### Mobile (<768 px) — out of scope Phase 1

Workspace not optimised for mobile. Login + case list + memo download work; full workspace shows a "best on tablet or desktop" notice.

---

## 15. State catalogue (every panel needs all of these)

| State | Pattern |
|-------|---------|
| Default | Loaded data, primary action visible |
| Loading | Skeleton screens with realistic placeholders, never spinner-only |
| Empty | Explanatory text + clear next action |
| Partial | Banner: "Some analyses incomplete. Showing what we have." + retry control |
| Error | Honest about what failed, what works, what to do |
| Stale | Data is older than X — banner offering re-run |
| Permission denied | Explanation of why + how to request access |
| Tier-blocked | Feature-tier policy blocked this view — show what would be needed to enable |

---

## 16. Accessibility checklist (WCAG 2.2 AA)

- [ ] Colour-blind-safe palette throughout (all recommendation states distinguishable in greyscale).
- [ ] All interactive elements keyboard-reachable in logical tab order.
- [ ] Focus visible on every interactive element.
- [ ] Form fields have explicit labels (no placeholder-as-label).
- [ ] Error messages are programmatically associated with fields.
- [ ] All images / icons have alt text or `aria-label`.
- [ ] Charts have text-equivalent representations (data table view toggle).
- [ ] No information conveyed by colour alone (always paired with shape / text).
- [ ] Heading hierarchy follows document structure (no skip-levels for styling).
- [ ] Skip links for keyboard users.
- [ ] Sufficient contrast (4.5:1 normal text, 3:1 large text and UI components).
- [ ] Screen-reader testing with NVDA + VoiceOver before each release.
- [ ] axe-core CI gate; Pa11y monthly audit.

---

## 17. Performance budgets

| Metric | Budget | Where measured |
|--------|--------|----------------|
| LCP (Largest Contentful Paint) | < 2.0s P95 | Lighthouse CI on every PR |
| INP (Interaction to Next Paint) | < 200ms P95 | RUM via PostHog |
| CLS (Cumulative Layout Shift) | < 0.1 | Lighthouse CI |
| Initial JS bundle | < 250 KB compressed | webpack-bundle-analyzer in CI |
| Workspace cold load | < 4s P95 | Synthetic monitoring |
| Recommendation refresh after slider | < 600ms P95 | RUM |

---

## 18. Voice & tone (UI copy guidance)

- **Direct, not hedged.** "Recommendation: SETTLE" — not "Our model suggests you may wish to consider settling."
- **Quantified, not vague.** "P(win) 41% (90% CI 33-49%)" — never "moderate likelihood."
- **Calibrated, not boastful.** "Statistical estimates only. Not legal advice." — appears on every screen.
- **Client-facing copy is more cautious than internal copy.** PDF memo softens phrasing slightly vs the workspace.
- **No anthropomorphism of the AI.** "The model predicts" not "JudicialPredict thinks." This product is statistical inference, not judgment.
- **Numbers always have units and reference points.** "$1.4M" — never raw numbers without context.
- **Plain English over jargon for partner-facing surfaces.** "Materiality finding borderline" — not "fuzzy MF score below crisp threshold."

---

## 19. Open design questions for review

1. **Recommendation pill colour mapping.** Currently sketched as filled / hollow / striped. Final colour palette TBD with the brand identity work — but never red-vs-green only.
2. **Where does CVaR live in the partner-default view?** Currently shown next to EV — partners new to the concept may need an inline tooltip "What is CVaR?"
3. **How loud is the federated-learning UX?** Some firms will want it visibly opt-in, others will want it quiet. Default opt-out is in the spec; the admin toggle is prominent — that may be enough.
4. **Counterfactual UI density.** Strategic counterfactuals as checkboxes seems right; but combined-effect rendering may need richer interactions (e.g., scenario-builder modal for arbitrary combos).
5. **PDF memo tone.** Cravath-grade typography vs more accessible style — depends on target firm size.
6. **Compliance disclosure prominence.** Footer on every screen vs dedicated Compliance tab only — current sketch does both, may be excessive.
7. **Empty state for new firms.** First-case onboarding flow — separate guided experience, or just standard intake with helper text? Strong recommendation: dedicated guided first-case mode.

---

**Next steps**

1. Review with product + frontend leads.
2. User-research validation: 5-7 partner interviews testing the Summary + Bargaining tabs.
3. High-fidelity mockups in Figma after signoff.
4. Component-level specs for the design system.
5. Prototype the Outcome-tab sensitivity sliders early — that interaction is the linchpin of the whole UX.
