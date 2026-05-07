# JudicialPredict — UX Research Interview Plan

**Sprint:** 1
**Plane issue:** JP-17
**Owner:** UX Researcher (NEW HIRE) + Senior Product Designer (NEW HIRE)
**Status:** Draft for review by gigforge-pm + Operations Director
**Date:** 2026-05-07

> Seeded by PM during agent-stall recovery; UX Researcher to review, refine recruiting strategy, and execute. The `gigforge-ux-designer` agent will iterate on this plan once their tool-use loop is debugged on the available models.

## 1. Goal

Validate the v2.13 spec assumptions about how partners, associates, paralegals, ops staff, and compliance officers actually evaluate cases. Produce five tested personas, an information-architecture sanity check on the workspace tab structure, and a list of usability concerns that pre-launch design must address.

## 2. Method

**Contextual inquiry interviews** — 60-minute remote sessions on Google Meet (or in-person where pilot firms are willing). Mix of "show me how you currently decide settle vs trial" walk-throughs and structured questions on the v2.13 workspace concept. Recorded with consent; transcribed via Whisper; analyzed with affinity-mapping in Figma FigJam.

## 3. Sample

**12-15 interviews across 5 personas** (target distribution; flex by recruit-availability):

| Persona | Target count | Recruiting source | Notes |
|---------|--------------|-------------------|-------|
| Partner (decision authority, time-poor) | 3-4 | Pilot-firm partners + GigForge network referrals | Primary persona — must skew slightly heavier |
| Associate (does the case work) | 3-4 | Pilot-firm associates + LinkedIn AmLaw 100 outreach | Heaviest workspace user |
| Paralegal | 2-3 | Pilot firms; backup via NALA membership outreach | Intake + document workflows |
| Ops / Litigation Support | 2 | Pilot firms; backup via ILTA member networks | Firm admin, billing, partner-API integration |
| Compliance Officer | 2 | Larger firms with formal compliance functions | Less critical for Sprint 1; may slip to Sprint 2 |

**Inclusion criteria:** practicing in or supporting US Federal / California / New Jersey civil + criminal + bankruptcy matters; at least 2 years' experience; at least one case in scope per quarter; firm size ≥ 10 attorneys (smaller firms are out of scope for the pilot).

**Exclusion criteria:** prosecutors-only or judge-side respondents (Phase 1 framing is firm-side); attorneys whose primary practice is a jurisdiction we don't cover.

## 4. Recruiting strategy

**Tier 1 — pilot-firm direct.** As soon as Operations confirms the pilot-firm shortlist, request 5-7 introductions across the persona mix. Offer: $250 honorarium per partner / associate session, $150 paralegal / ops session.

**Tier 2 — GigForge network referrals.** Cold-warm intros via Alex Reeves's existing network. Same honorarium.

**Tier 3 — LinkedIn outreach.** Targeted by AmLaw 100 / 200, NJSBA, CSBA membership lookups. Lower yield but valuable for pilot-firm-independent perspective.

**Tier 4 — bar-association partnerships.** ILTA, NJ State Bar, CA State Bar — request a shortlist of firms willing to participate in software UX research in return for early access.

Recruit screener questionnaire (10 questions, 3 minutes) gates participation. Screener confirms inclusion criteria, persona fit, conflict-of-interest absence (we are not surveying attorneys at firms with active disputes against Operations / GigForge / pilot firms).

## 5. Schedule (4-week window)

**Week 1 (May 12-16):** finalize screener; launch outreach across all four tiers; begin scheduling. Target 4-5 interviews booked by end of week.

**Week 2 (May 19-23):** conduct first 4-5 interviews. PM + UX Researcher pair-observe each. Same-day debrief into shared notes.

**Week 3 (May 26-30):** conduct next 4-5 interviews. First-round affinity mapping; persona drafts begin.

**Week 4 (Jun 2-6):** conduct final 3-5 interviews. Full affinity mapping; persona finalization; usability findings compiled.

**End of week 4:** 5 personas published in `/opt/ai-elevate/gigforge/projects/judicialpredict/design/personas/` (one Markdown file per persona); usability findings published in `design/ux-research-findings-2026-w22.md`; presented at Sprint 2 review.

## 6. Interview script outline

**Pre-interview (5 min):** consent form, recording confirmation, intro to project framing ("we're building a settle-vs-trial decision-support tool; we want to learn how you actually decide today before showing you our concept").

**Section A — Current behavior (20 min):** walk us through the last case you decided to settle. Walk us through the last one you decided to take to trial. What information did you weigh? What did you wish you had? Whose opinion did you rely on?

**Section B — Concept reaction (20 min):** show the v2.13 wireframes (`judicialpredict-wireframes.md` §5 Summary tab + §6 Outcome tab + §8 Bargaining tab + §10 Compliance tab). Reaction to each panel. What's missing? What's noise? Does the recommendation card feel defensible to your client?

**Section C — Workflow + tooling (10 min):** how does this fit into your case-management system (Clio / MyCase / NetDocs)? What would block adoption at your firm? Who needs to approve the purchase?

**Section D — Compliance + ethics (5 min):** comfort level with the demographic / personality / ideology features for judges and attorneys; concerns about disparate-impact exposure; how would you want the firm's compliance posture surfaced?

**Wrap (5 min):** open feedback; permission to follow up.

## 7. Recording, consent, and data handling

- **Consent form** signed before each interview. Specifies: recording purpose (UX research only), retention (12 months max), no sharing with third parties, right to withdraw.
- **Recording** via Google Meet's built-in record (audio + video) or Zoom; backed up to MinIO under `/projects/judicialpredict/research/recordings/` with retention timer.
- **Transcription** via Whisper (local) — never sent to external services.
- **Anonymization** — partner / firm names redacted in published findings; persona writeups use composite synthetic representatives (no 1:1 mapping to interviewees).
- **GDPR / CCPA compliance** — interviewees can request data deletion at any time; opt-out registry maintained in `design/research-opt-outs.md`.

## 8. Output deliverables

By Sprint 2 review (June 6, 2026):

1. **Five persona Markdown files** under `design/personas/` — Partner, Associate, Paralegal, Ops, Compliance Officer.
2. **UX research findings document** — `design/ux-research-findings-2026-w22.md` — top 10 usability issues, IA validation results, voice/tone calibration, prioritized backlog of concept changes for Sprint 2+.
3. **Interview transcripts** archived (anonymized) under `design/research/transcripts/` for future reference.
4. **Affinity map** (Figma FigJam link) embedded in findings doc.
5. **Demo-ready 1-slide summary** of "what we learned" for the sprint review.

## 9. Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Pilot firms not yet confirmed → recruiting delayed | Medium | Tier 2 + 3 + 4 sources can supplement before pilots are signed |
| Partners refuse to participate due to time | High | $250 honorarium + flexible scheduling + offer of summary deliverable |
| Findings invalidate spec assumptions late in Sprint | Low | Sprint 2 has explicit slack for IA refactors; spec is a living doc |
| Compliance officers as a persona prove too rare to recruit | Medium | Acceptable to defer this persona to Sprint 2 if needed |

## 10. Success metrics

- 12+ interviews completed by end of Week 4.
- All 5 personas published with grounded evidence (≥ 2 interviewees informing each).
- ≥ 3 IA-affecting findings (workspace tab structure, panel hierarchy, intake flow) validated or invalidated explicitly.
- Sprint 2 design backlog populated with research-grounded items.

---

*This plan was seeded by the PM during agent-stall recovery. UX Researcher to review and execute. The `gigforge-ux-designer` agent will be re-engaged for sprint-2 persona-writing dispatches once tool-use is reliable on available models.*
