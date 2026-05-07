# PENDING DISPATCH — gigforge-pm

**Status:** Auth-blocked. Dispatch attempted 2026-05-07 11:08 UTC via `openclaw agent --agent gigforge-pm` returned HTTP 401: "missing or invalid Authorization: Bearer <token> header".

**Pre-existing issue:** The same 401 has been hitting other dispatch paths since at least 2026-05-05 (see `/var/log/openclaw/shared/scout-auto.log` — `gigforge-scout` cron has been failing the same way every 2h for 2+ days).

**Fix required (before this dispatch can fire):**
1. Repair the gateway auth bridge between `openclaw agent --agent ...` CLI and the local gateway at `127.0.0.1:18789`. The gateway expects `Authorization: Bearer b8f1afb652fe20adae78b3e2c2a31917f16d9738470733ea` per `~/.openclaw/openclaw.json:gateway.auth.token`, but the CLI is not sending it. Possible fixes:
   - Restart gateway with token explicitly set in env (`OPENCLAW_GATEWAY_AUTH_TOKEN`).
   - Reinstall openclaw CLI to a version that reads the gateway token from config correctly.
   - Patch the CLI to pass `Authorization: Bearer ${gateway.auth.token}` from openclaw.json.

**Once auth is repaired, run:**

```bash
cd /home/aielevate
env -u CLAUDECODE openclaw agent --agent gigforge-pm \
  --message "$(cat /opt/ai-elevate/gigforge/projects/judicialpredict/PENDING-PM-DISPATCH.txt)" \
  --thinking low --timeout 600 \
  >> /opt/ai-elevate/gigforge/projects/judicialpredict/.dispatch.log 2>&1
```

The dispatch text body is in `PENDING-PM-DISPATCH.txt` in this same directory.

**Pre-flight done before the auth block:**
- Plane project JudicialPredict (id `92ad0116-cbac-4975-ac87-4ea820c0be96`) created with 23 epics labelled by owning agent.
- Sprint 1 cycle "Sprint 1 — Foundation + Methodology Rollout" (2026-05-07 → 2026-05-21) created.
- Workflow states (Backlog → Ready → In Progress → In Review → In Testing → In Staging → In Production → Cancelled) configured.
- Project workspace at `/opt/ai-elevate/gigforge/projects/judicialpredict/` initialised with spec, wireframes, KICKOFF.md, ADR / sprint-board / report / handoff / memory-bank dirs.
- Intake folder at `/opt/ai-elevate/gigforge/inbox/programming/2026-05-07-judicialpredict/` populated with spec + wireframes + KICKOFF.md.
- RAG (gigforge/engineering collection): spec + wireframes + KICKOFF ingested (135 + 63 + ~30 chunks).
- GigForge website portfolio entry live at `http://localhost:4091/portfolio/judicialpredict` (rebuild completed 2026-05-07 ~10:33 UTC).
- Sync cron `*/30 * * * * /opt/ai-elevate/gigforge/projects/judicialpredict/jp-sync.sh` active — RAG re-ingests changed docs, mirrors Plane issues + cycles, generates KG snapshot every 30 min.
- Local repo at `/home/bbrelin/judicialpredict` (Braun's workstation).
