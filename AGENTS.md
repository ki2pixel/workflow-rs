# AGENTS.md

Rust rewrite of the Python `workflow_mediapipe` pipeline. The Python app at
`/home/kidpixel/workflow_mediapipe` is the legacy production reference and the
rollback path; Rust artifacts must match its outputs (JSON comparator tolerance
`1e-6`, media/CSV as applicable). `parity/DRIFT.md` lists intentional quirks;
do not "clean them up". Start with `README.md` for scope/status and
`docs/CONTRAT-ENVIRONNEMENT.md` for the HTTP/artifact contract.

## Agents, skills and memory

- Detailed rules: `.agents/rules/` (coding standards, tests, memory protocol,
  security).
- Skills: `.agents/skills/`; routing matrix in
  `.agents/rules/skills-integration.md`.
- Workflows: `.agents/workflows/`.
- Project memory: `memory-bank/`; read `activeContext.md` first.
- Memory-bank protocol: use absolute paths under
  `/home/kidpixel/workflow-rs/memory-bank/`; relative `memory-bank/...` paths
  resolve under the `fast-filesystem` MCP root (`/home/kidpixel/kimi-proxy`),
  outside this repository.

## Layout

- `crates/workflow-core`: typed TOML config (`deny_unknown_fields`), resolved
  paths, hardware lock.
- `crates/workflow-schema`: JSON schemas and Python-compatible decimal rounding
  (`round.rs`).
- `crates/workflow-steps`: STEP1–STEP8 implementations (modules `step1` …
  `step8`) plus `post/` (A5/A6 post-traitement after STEP8: `_assist.ecproj`,
  `post.json`, `review.json`, `audit.json`).
- `crates/workflow-cli`: operational entrypoint for all eight steps; the
  server and scripts invoke it. There is no Python fallback.
- `crates/workflow-server`: axum server on `:5004`. It serializes runs and
  spawns `server.workflow_cli`; it does not link step logic itself. Cache,
  stats, diagnostics and `/api/audio/*` routes intentionally stay in Python
  legacy (D-015); do not port them speculatively.
- `crates/workflow-ingest`: webhook monitor, URL normalization, SQLite download
  history, resumable R2/Dropbox downloads.
- `crates/workflow-sync`: merges Python/Rust history, STEP1 list and STEP7
  archives before a backend switch.
- `crates/workflow-bridge`: After Effects JSON bridge (`cuts` and manifest
  modes). The CEP panel still calls Python (D-022).
- `crates/workflow-tpu`: dynamic TFLite/Edge TPU FFI plus the spike binary.
- `assets/web` is a copy of the legacy frontend; `assets/models/*` binaries and
  `vendor/*.so*` are gitignored. Only the model manifest/README are versioned.

## Commands

```sh
cargo test --workspace                                    # 315 unit tests, no hardware
cargo test -p workflow-steps step6 --lib                  # filter one step/test
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p workflow-server -p workflow-cli  # required by launchers
cargo run -p workflow-cli -- config check                 # no secrets needed
cargo run -p workflow-cli -- post run --folder /tmp/projet  # A5/A6 only
```

- There is no CI; run fmt + clippy + tests manually before finishing.
- MSRV is Rust 1.86 (edition 2021). Use `--release` for STEP2/3/4, STEP8 and
  the server; `config/config.toml` points the server at
  `target/release/workflow-cli`.
- `--config` is global; its default `config/config.toml` is relative to the
  current directory, so pass it explicitly when running from `/tmp`.
- Full server preflight (secrets, Python stopped, tools, TPU libs) is
  `./scripts/switch-rust.sh check` after
  `set -a; source config/launcher.env; set +a`.
- `scripts/start-rust.sh` is interactive (generates secrets, opens Firefox,
  tails logs). For agent/non-interactive use, call
  `scripts/switch-rust.sh {check,start,stop,status}`.

## Safety rails

- Treat `runtime/` as live production data, not scratch space. It contains the
  real SQLite history, archives, logs and PID file. Do not run experiments,
  `retention apply`, `step1 run`, `step7 finalize` or `history import` against
  it unless the task is explicitly about production state.
- `config/config.toml` currently enables the real webhook (`enabled = true`,
  `dry_run = false`). Starting `workflow-server` with it polls the real webhook
  and downloads new archives. Check `[webhook]` and use a copied config with the
  monitor disabled for tests.
- Destructive defaults: STEP1 deletes processed archives, STEP7 deletes source
  projects (`--no-delete` keeps them) and writes to `/mnt/cache` by default,
  `retention apply` deletes files, and `sync --apply` rewrites both Python and
  Rust databases/lists. For experiments pass `--work-dir`, `--output-dir`,
  `--archives-dir` and `--logs-dir` explicitly; use `step7 finalize --dry-run`,
  `retention plan`, or no `--apply` first. STEP1 has no dry-run mode.
- Never run both webhook monitors. With `webhook.enabled = true`, only one
  backend may run (Python `:5003`, Rust `:5004`), and `switch-rust.sh` refuses
  to start Rust while Python answers. Even with the monitor off, never run
  hardware STEP2/3/4 on both stacks at once (they use separate locks).
- STEP8 always runs the A5/A6 post-traitement when it succeeds: it writes a
  second `<stem>_assist.ecproj` and `docs/post/<stem>_assist/` artifacts. The
  raw project is never modified. Use `--no-post` to disable it, or
  `workflow-cli post run --report-only` to compute reports without publishing
  an assisted project. Do not run either against `runtime/` or `/mnt/cache`
  for experiments.
- Before a switch: stop both servers (sync refuses if either `:5003` or
  `:5004` is listening), run `./scripts/sync-backends.sh` (dry-run), then
  `--apply`. `scripts/start-rust.sh` does this automatically when the Rust
  monitor is enabled.
- `/mnt/cache` is the shared finalized output. `retention` never deletes it;
  do not write or delete there from tests.
- `config/config.toml` is versioned and contains machine-specific absolute
  paths (`/home/kidpixel/...`, `/mnt/cache`). Do not commit experimental path
  edits. `WORKFLOW_PATHS_BASE` only changes `paths.base`; explicit
  `projects`/`archives`/`logs`/`downloads` still win, so use a full temp TOML
  for isolation.
- The server rejects `:5003`, a history DB outside `paths.base`, the Python
  hardware lock `/tmp/workflow-hw.lock`, and default/`dev-` worker tokens; a
  non-loopback `server.host` also requires `server.allow_remote = true`.
- Do not edit `/home/kidpixel/workflow_mediapipe`; it is the production
  fallback. The only intended writer is an explicit `sync --apply`.
- Operational detail lives in `docs/SYNC-BACKENDS.md`, `docs/RETENTION.md` and
  `docs/SWITCH-PUBLIC.md`; read the relevant one before changing deployed state.
- No native desktop client: the Tauri/GTK/WebKit PoC was abandoned (D-028).
  The UI is `assets/web` served by the server; do not reintroduce Tauri, GTK,
  WebKit or zbus dependencies without a new decision.

## Parity contracts

- Unit tests are inline `#[cfg(test)]`; there is no integration-test directory.
  The suite does not exercise models, GPU or Coral, so a green suite does
  **not** prove step-output parity.
- JSON output is contractual: key insertion order matters (`serde_json` uses
  `preserve_order`), `_audio.json` writes `fps`/`total_frames` before
  `frames_analysis`, and writers use temp-file + rename. Preserve these when
  touching step5/step6/step4 output.
- Use `workflow_schema::round::{round2, round4, round_to}` for Python-compatible
  rounding; `f64::round` changes half-value behavior.
- Keep documented quirks, e.g. STEP1's scan ignores `.tar.gz`/`.tar.bz2`/
  `.tar.xz` (D-016), STEP3 no-audio exits 0 with no artifact (D-002), and STEP4
  requires `<stem>.csv` and `_audio.json` by default (D-004).
- `parity/` contains the Python baseline harness: it reruns **legacy Python**
  commands and compares them with frozen fixtures; it does not invoke the Rust
  binaries. Corpus/fixtures are gitignored and machine-local. Its venv is
  `../workflow_mediapipe/coral_env/bin/python`, not `coral_env/` in this repo.
- Real Rust comparisons are `scripts/shadow-step56.py`,
  `scripts/validate-lot5.py` and `scripts/validate-step1.py`. They read
  production under `/mnt/cache` (or the legacy repo) and write to `/tmp`; never
  write under `/mnt/cache`.
- Do not resurrect Windows/After Effects bridge validation: production targets
  Linux and the CEP panel remains on Python (D-022).

## Prerequisites

- Models: `python3 scripts/import-models.py` copies from
  `/home/kidpixel/workflow_mediapipe` and rewrites the manifest. A fresh
  checkout has no model binaries.
- TFLite C API: `scripts/build-tflite-c.sh` produces
  `vendor/libtensorflowlite_c.so`. STEP3/4 also need
  `vendor/libedgetpu.so.1` and an accessible `/dev/apex_0`.
- STEP2 needs an ONNX Runtime 1.23 CUDA library (path in `[steps.step2]`) and a
  CUDA GPU. CPU devices are diagnostic/test paths, not the production config.
- STEP1–STEP4 need `ffmpeg`/`ffprobe`; RAR archives need `unrar`; STEP8 needs
  `effectcraft-cli` 0.13. `scripts/switch-rust.sh check` reports missing items.

## Conventions

- Project docs, code comments, logs and user-facing messages are in French;
  keep that convention when editing them.
- Add step behavior in the relevant `crates/workflow-steps/src/stepN` module
  (directory or `stepN.rs`), CLI wiring in `crates/workflow-cli/src/main.rs`,
  and server command mapping in `crates/workflow-server/src/catalog.rs`.
- Read the relevant `docs/LOT-*.md` before changing a step; it records the
  validated scope, limits and parity evidence.
