---
trigger: model_decision
description: Détection et routage des skills de workflow-rs selon les motifs de la demande
globs:
---

# Matrice d'intégration des skills

## Détection

| Motifs | Skill | Priorité |
| --- | --- | --- |
| `lancer`, `étape`, `pipeline`, `séquence`, `serveur`, `5004`, `workflow-cli`, `switch`, `sync` | `workflow-operator` | 1 |
| `bug`, `crash`, `erreur`, `lent`, `performance`, `profiler`, `régression` | `debugging-strategies` | 1 |
| `doc`, `README`, `documentation`, `guide`, `TL;DR` | `documentation` | 1 |
| `tâche`, `backlog`, `plan`, `roadmap`, `décomposer` | `shrimp-task-manager` | 1 |
| `réflexion`, `logique`, `architecture`, `analyser`, `décision` | `sequentialthinking-logic` | 1 |
| `test`, `cargo test`, `clippy`, `fmt`, `couverture` | `tests-suite-guardian` | 2 |
| `audio`, `diarisation`, `yamnet`, `ecapa`, `ahc`, `step3`, `locuteur` | `step3-audio-orchestrator` | 2 |
| `tracking`, `blazeface`, `facemesh`, `efficientdet`, `one-euro`, `step4`, `locuteur visuel` | `step4-tracking-ops` | 2 |
| `coral`, `tpu`, `apex_0`, `gasket`, `aspm`, `sram`, `libedgetpu` | `coral-tpu-system-orchestration` | 2 |
| `tflite`, `int8`, `quantization`, `edgetpu_compiler`, `manifeste`, `cocompilé` | `coral-tpu-model-compiler` | 2 |
| `médiane`, `hangover`, `clustering`, `cosinus`, `one-euro`, `arrondi` | `coral-tpu-hybrid-algorithms` | 2 |
| `onnx`, `cuda`, `ort`, `cudnn`, `tf32`, `transnet`, `step2`, `vram` | `onnx-runtime-cuda-expert` | 2 |
| `tokio`, `rayon`, `async`, `canal`, `verrou`, `concurrence`, `sémaphore`, `sérialisation` | `rust-concurrency-orchestrator` | 2 |
| `webhook`, `csv`, `sqlite`, `download_history`, `url`, `dropbox`, `r2`, `ingestion` | `csv-monitoring-sme` | 2 |
| `ae`, `after effects`, `jsx`, `_ae.json`, `_tracking.json`, `cuts`, `bridge` | `after-effects-json-contract` | 2 |
| `workflow-bridge`, `manifest_path`, `analyze_manifest`, `cep` | `after-effects-bridge` | 2 |
| `diagnostic`, `préflight`, `santé`, `environnement`, `modèle`, `vérification` | `pipeline-diagnostics` | 3 |
| `timeline`, `frontend`, `ui`, `logs`, `overlay`, `minijinja` | `frontend-timeline-designer` | 3 |
| `overlay`, `focus`, `auto-ouverture`, `logs` | `logs-overlay-conductor` | 3 |
| `json`, `jsonpath`, `masse`, `clé`, `valeur`, `jq` | `json-mcp-expert` | 3 |
| `docs`, `mise à jour`, `README`, `lot`, `contrat` | `workflow-docs-updater-plus` | 3 |

## Chargement automatique

Lorsqu'un motif est détecté, charger :

```text
.agents/skills/<skill>/SKILL.md
```

Pour les skills avec ressources, charger ensuite le fichier utile :

- `workflow-operator` → `resources/step_command_matrix.md`
- `pipeline-diagnostics` → `resources/env_health_checklist.md`
- `tests-suite-guardian` → `resources/test_execution_matrix.md`
- `workflow-docs-updater-plus` → `resources/docs_sync_checklist.md`
- `step3-audio-orchestrator` → `resources/audio_triage.md`
- `step4-tracking-ops` → `resources/tracking_diagnostics.md`
- `csv-monitoring-sme` → `resources/sqlite_triage_commands.md`
- `frontend-timeline-designer` → `resources/timeline_design_tokens.md`
- `logs-overlay-conductor` → `resources/overlay_focus_playbook.md`

## Multi-skills

Pour une demande complexe, combiner les skills par priorité : d'abord
`workflow-operator`, puis la skill d'étape ou de domaine, puis les skills
génériques (tests, docs, debugging). Ne pas charger une skill dont le sujet est
hors périmètre Rust sans le signaler.

## Hiérarchie

1. `workflow-operator` pour l'exploitation.
2. Skills d'étape ou de domaine (STEP3, STEP4, Coral, ONNX, ingestion...).
3. `codingstandards.md` et les règles du dépôt.
4. `docs/LOT-*.md`, `docs/CONTRAT-ENVIRONNEMENT.md`, `parity/DRIFT.md`.
5. Skills génériques (documentation, debugging, JSON, planification).

## Règle de frontière

Les skills `after-effects-bridge` et `after-effects-json-contract` documentent
la frontière avec le legacy Python/JSX/CEP. Ils ne doivent jamais conduire à
modifier le panneau, les scripts JSX ou le dépôt Python sans décision explicite
(D-022).
