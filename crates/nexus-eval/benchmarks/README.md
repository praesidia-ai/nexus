# Nexus Token-Efficiency Benchmark

Nexus claims it uses **≥ 3.75× fewer tokens than Claude Code on the
same blind-graded tasks**, on the same Claude Sonnet 4.5 model, at
equivalent quality. This directory holds the reproducible suite and
scoreboard that backs the claim.

## Suite layout

```
benchmarks/
  v1/                        # the current frozen suite
    001-crud-feature.json
    002-bugfix.json
    ...
```

Every task is a single JSON file with this shape:

```json
{
  "id": "crud-001",
  "category": "crud",
  "prompt": "...natural-language description of the task...",
  "rubric": "...what a 5/5 solution must do..."
}
```

A freeze bumps the directory: `v1` → `v2`, never editing old tasks in
place. Comparisons across versions are always explicit about which
version they used.

## Running the suite

The runner is model-client-agnostic — it calls an injected closure
for each task. Nexus's own runner lives at
`crates/nexus-bench/src/token_bench_main.rs` and drives both
`nexus /oneshot` and a reference Claude-Code baseline.

## Output

The aggregate output is a `ScoreboardRun` (see
`crates/nexus-eval/src/token_bench.rs`) serialised into
`web/public/bench/scoreboard.json`. The `/bench` page on the website
reads that file and renders the comparison table. Regenerating the
scoreboard is a CI step; the URL never lies because the file always
matches the committed suite.
