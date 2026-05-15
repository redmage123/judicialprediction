# Mutation testing runbook

We use [`cargo-mutants`](https://mutants.rs) on the four functional-core
crates to surface mutations the test suite fails to catch.  The same
sweep runs on two cadences:

| Where           | When               | Source of truth                              |
|-----------------|--------------------|----------------------------------------------|
| Hetzner host    | Mon 06:00 UTC      | `rust/scripts/mutants-weekly.cron`           |
| GitHub Actions  | Mon 10:00 UTC      | `.github/workflows/cargo-mutants-weekly.yml` |

Both invocations call the same `rust/scripts/mutants-weekly.sh` script
and diff against the pinned baseline at `rust/.mutants-baseline.json`.

## Baseline semantics (S6.11)

The baseline records, per crate, how many mutants the current test suite
catches / misses / can't compile.  Each week's sweep diffs against it.

* **Pinned by default.** The script does not rewrite the baseline file
  on its own.  Update it deliberately as part of the PR that adds the
  test which closes a survivor (see "Updating the baseline" below).
* **Alerts only on regression.** When `missed` is unchanged or smaller,
  the summary is appended to `/var/log/jp-mutants-weekly.log` (or
  printed to stdout in CI) — no Slack ping.  When any crate has more
  survivors than the baseline, the script posts a Slack message via
  `SLACK_WEBHOOK_URL` AND exits non-zero so the GitHub Actions job
  badge goes red.
* **Improvements are silent.**  A new caught mutation that lowers
  `missed` below the baseline shows up in the log summary; the
  operator updates the baseline (see below) the same day.

## Updating the baseline

After landing a PR that adds tests killing a previously-surviving
mutant:

```bash
cd rust
MUTANTS_UPDATE_BASELINE=1 ./scripts/mutants-weekly.sh
git add rust/.mutants-baseline.json rust/CARGO_MUTANTS_BASELINE.md
git commit -m "chore: refresh cargo-mutants baseline after <ticket>"
```

The script writes the new counts into the JSON; commit them in the
same PR as the test additions so reviewers see the survivor-count
delta alongside the test.

If a survivor is intentionally added (e.g., a mutation that flips an
intentionally permissive default — rare), include a section in
`rust/CARGO_MUTANTS_BASELINE.md` explaining the decision before
bumping the JSON.

## Slack alert payload

The Slack message body is the full per-crate summary that lands in the
log, prefixed with `## JudicialPredict — Weekly Mutation Test Report
(<UTC timestamp>)`.  The `🔴 REGRESSION: missed N (was M)` lines are
the headline — open the GitHub Actions run from the link in the
workflow run summary to inspect the `mutants-out` artifact.

## GitHub secrets

| Secret              | Used by                                  |
|---------------------|------------------------------------------|
| `SLACK_WEBHOOK_URL` | `.github/workflows/cargo-mutants-weekly.yml` (also `web-a11y.yml`'s a11y gate uses its own webhook through a different secret if needed). |

The webhook posts to the `#judicialpredict-mutants` Slack channel.
Rotate the webhook URL whenever an operator with channel access leaves
the project.

## Manual one-shot

For a sanity run before landing test work:

```bash
cd rust
cargo install cargo-mutants --version "^27" --locked
./scripts/mutants-weekly.sh
```

The script is idempotent and safe to run from any working tree.  Each
mutant invocation has a 1800-second hard timeout; the four crates
together complete in roughly 30 minutes on the Hetzner host.
