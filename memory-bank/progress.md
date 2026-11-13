# Suivi de progression

## Statut actuel

| Composant | État au 2026-11-13 | Référence |
| --- | --- | --- |
| Pipeline STEP1 → STEP8 | Huit étapes portées et validées ; STEP7 publie et archive, STEP8 est headless (`effectcraft-cli`). | `docs/lots/LOT-*.md`, `parity/DRIFT.md` |
| Post-traitement A5/A6/A7 | A5/A6 en production depuis D-032 ; A7 résout les missions, la banque centrale, le masque QA MOV (D-064 → D-067) et reprend un asset local corrompu sur la banque (D-069). Brut jamais modifié ; `_assist.ecproj` toujours publié. | `docs/lots/LOT-9-POST-STEP8.md`, `docs/lots/LOT-12*.md` |
| Bypass SDM/LPHDF | Bascule directe opt-in pour les profils 1920×1080 25 fps H.264 ; préfixes `M6 - SDM - `, `SDM - ` et `W9 - LPHDF`. | D-034, D-035, D-061 |
| Ingestion | Webhook Rust, normalisation d'URL, historique SQLite, téléchargements R2/Dropbox ; un seul moniteur actif. | `docs/lots/LOT-8-INGESTION.md` |
| Serveur `:5004` | axum, une étape à la fois (409), arrêt `stopping` puis SIGTERM/SIGKILL ; Python `:5003` arrêté, rollback disponible. | `docs/lots/LOT-5-ORCHESTRATOR.md` |
| QA read-only | Schéma `1.5.0`, cache 12, masque des `.mov` alpha (12–24 frames), DINOv2 opt-in. | D-049, D-051, D-067 |
| Parité | STEP2/3/4 à `1e-6`, shadow `162/162`, CSV/JSON contractuels préservés. | `parity/DRIFT.md`, `docs/validation/VALIDATION-STEP1-8.md` |
| Tests et outillage | 675 tests listés (673 exécutés + 2 ignorés), `fmt` et `clippy -D warnings` verts ; pas de CI. | `AGENTS.md` |

- Backend de production : Rust `:5004` headless via `scripts/switch-rust.sh` ;
  interface web legacy `assets/web` uniquement (D-028).
- Webhook activé dans la configuration depuis le 2026-10-23 ; il ne tourne que
  si le serveur est démarré.
- Verrou matériel : `/tmp/workflow-rs-hw.lock` ; runtime `runtime/`, sortie
  finalisée `/mnt/cache`.

## Synthèse des Lots

| Lot | Période | Livrable | Documentation | Décision |
| --- | --- | --- | --- | --- |
| 1 | 2026-10-22 | STEP5/6/7/8, bridge AE, shadow | `LOT-1-STEP5.md` … `LOT-1-SHADOW.md` | D-001 → D-027 (écarts) |
| 2 | 2026-10-22 | STEP2 TransNetV2 ONNX CUDA | `LOT-2-STEP2.md` | D-009, D-011 |
| 3 | 2026-10-22 | STEP4 tracking Coral | `LOT-3-STEP4.md` | D-004, D-008 |
| 4 | 2026-10-22 | STEP3 audio YAMNet/ECAPA/AHC | `LOT-4-STEP3.md` | D-002, D-003 |
| 5 | 2026-10-23 | Serveur axum `:5004`, séquence, arrêt | `LOT-5-ORCHESTRATOR.md` | D-013, D-025, D-027 |
| 6 | 2026-10-23 | STEP1 extraction et normalisation | `LOT-6-STEP1.md` | D-016, D-017 |
| 7 | 2026-10-23 | Intégration STEP2 → STEP8 | `LOT-7-INTEGRATION.md` | D-023, D-026 |
| 8 | 2026-10-23 | Ingestion webhook, SQLite, R2/Dropbox | `LOT-8-INGESTION.md` | D-018 → D-021 |
| 9 | 2026-10-27 → 2026-11-09 | Post-traitement A5/A6, bypass, multi-préfixes | `LOT-9-POST-STEP8.md` | D-030 → D-035, D-050, D-061 |
| 10 | 2026-10-30 → 2026-11-02 | QA read-only, Stage 0/1, DINOv2 | `LOT-10-QA.md` | D-036 → D-049 |
| 11 | 2026-11-11 | Veto latéral A1 (pillarbox) | `LOT-11-A1-VETO-LATERAL.md` | D-063 |
| 12 | 2026-11-12 | Logos de mission A7 | `LOT-12-A7-MISSIONS.md` | D-064 |
| 12.1 | 2026-11-12 | Parseur, alias, `tail_overlay` | `LOT-12.1-PARSEUR-ALIAS-TAIL.md` | D-065 |
| 12.2 | 2026-11-12 → 2026-11-13 | Banque centrale de repli A7, reprise sur asset local corrompu (D-069) | `LOT-12.2-BANQUE-CENTRALE.md` | D-066, D-069 |
| 12.3 | 2026-11-12 | Benchmark, masque QA MOV, canari | `LOT-12.3-BENCHMARK-QA-MOV.md` | D-067 |

Études hors lots : YOLO/ONNX B0 → B2.1 (`docs/etudes/ETUDE-YOLO-ONNX-A2.md`,
D-060/D-061), corpus 248 (`docs/etudes/ETUDE-CORPUS-248-POST-STEP8.md`,
D-062/D-063), VS letterbox A1 (`docs/etudes/ETUDE-VSLETTERBOX-A1.md`, D-058),
Grounding DINO (`docs/etudes/ETUDE-GROUNDING-DINO-A2.md`, D-060).

## Prochaines étapes

1. **D-024** : diagnostiquer l'écart STEP4 Python sur `153 Camille` (4 977
   objets contre 11 853) sans modifier la production ; ne pas utiliser
   `--prefer-raw` sur ce projet.
2. **Bascule publique** : installer le reverse proxy (aucun nginx/caddy/
   cloudflared) après lecture de `docs/exploitation/SWITCH-PUBLIC.md`.
3. **Surveillance** : suivre le moniteur webhook, les téléchargements et
   `history stats` après quelques jours d'activité.
4. **A5/A6** : exposer `review.json` en lecture seule dans l'UI, recalibrer
   A3/A4 et traiter les 179/287 références manquantes.
5. **QA logos** : faire signer les 12 labels dev et valider DINOv2 sur des
   logos inconnus et des masques partiels avant activation par défaut.
6. **D-056** : décider de l'activation de R2 (`a1_defer_on_a3`) et R6
   (`a3_relaxed_gap`) ou les laisser opt-in.
7. **Documentation** : maintenir `README.md`, `docs/README.md`,
   `docs/lots/LOT-*.md`, `CONTRAT-ENVIRONNEMENT.md`, `parity/DRIFT.md` et la
   memory-bank ; les archives restent en lecture seule.

## Historique et archives

- [Archives de progression (octobre 2026)](file:///home/kidpixel/workflow-rs/memory-bank/archives/progress-archive-2026-10.md)
  (`memory-bank/archives/progress-archive-2026-10.md`) : corps narratif
  complet du 2026-10-22 au 2026-11-12, déplacé verbatim par D-068.
- Ce fichier ne conserve que le statut par composant, la synthèse des lots et
  les jalons. Fenêtre de détail : 90 jours ; au-delà, résumer et déplacer vers
  `memory-bank/archives/`.
- L'historique Python legacy reste dans
  `/home/kidpixel/workflow_mediapipe/memory-bank/archives/`.
