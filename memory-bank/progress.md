# Suivi de progression

## Statut actuel

- Migration Rust terminée pour les 8 étapes.
- Backend de production configuré : Rust `:5004` ; Python legacy `:5003` en
  rollback. Aucun serveur n'écoutait lors du bootstrap du 2026-10-24.
- Moniteur webhook Rust activé dans la configuration depuis le 2026-10-23
  (`webhook.enabled = true`).
- 233 tests unitaires, sans matériel.
- Chaîne STEP1 → STEP8 validée sur `1720 Camille.zip`.
- Shadow STEP5/6 `162/162` sur 20 projets.
- Sync réelle : 911 URL, 96 archives, 640 hash STEP7 (1,8 Go).
- **Aucune tâche active.**

## Chronologie de migration

| Lot | Dates | Commits principaux | Livré | Validation |
| --- | --- | --- | --- | --- |
| 0 | 2026-10-22 | `e83b0a9`, `4440718` | Workspace, contrat, harnais de parité, spike TPU, modèles | 23 cas de parité, spike stable |
| 1 | 2026-10-22 | `d792e06`, `6530ba6`, `afdc93f`, `43c5fbf`, `e195a89`, `c0b6482`, `97ef1bc` | STEP5 audio/vidéo, STEP6, bridge, STEP7, STEP8, rétention | Parité 244/245/249/250, shadow 57/57, projet 250 |
| 2 | 2026-10-22 | `50722f1`, `d8f2ad2` | STEP2 TransNetV2 ONNX CUDA | CSV identiques sur 4 cas, VRAM sous baseline |
| 3 | 2026-10-22 | `b0983af`, `b7d2cbc` | STEP4 Coral tracking | Parité `1e-6` sur `face_pan`, `real_244`, `real_245` |
| 4 | 2026-10-22 | `f5d9e81` | STEP3 audio YAMNet/ECAPA/AHC | JSON identiques sur `vox_2spk`, `real_244`, `real_245` |
| 5 | 2026-10-23 | `ea10057`, `b257868` | Serveur axum `:5004`, séquence, arrêt, tests concurrence | 12/12 artefacts, 409, SIGTERM |
| 6 | 2026-10-23 | `b349ca6`, `85f3289` | STEP1 extraction/normalisation | ZIP/RAR/TAR, `.mov`, archive réelle |
| 7 | 2026-10-23 | `5f51ee2`, `34c6b97` | Intégration STEP2 → STEP8, bascule contrôlée | Séquence réelle, parité STEP5/6 |
| 8 | 2026-10-23 | `73b9312`, `f1fc509`, `e0b14b8` | Ingestion webhook, SQLite, R2/Dropbox, routes CSV | 911 URL, audit 0 changement, 462 enregistrements |
| Webhook | 2026-10-23 | `07ae8da` | Activation du moniteur Rust | Base non vide, un seul backend |
| Lanceur | 2026-10-23 | `f827d53` | `start-rust.sh`, secrets, Firefox | Lancement réel |
| Sync | 2026-10-23 | `28287af`, `6b6e5e1` | Fusion Python ↔ Rust | 911/96/640, 1,8 Go |
| Shadow | 2026-10-23/24 | `01fbbf2`, `b7b594b` | Corpus 153–159 et 1720 | 162/162 sur 20 projets |

## Détails des validations

### STEP1 → STEP8

- Archive : `/mnt/cache/old/1720 Camille.zip` (1,7 Go, 4 MP4, 1 PNG).
- Configuration isolée sous `/tmp/opencode/step1-8`.
- Résultat : 8/8 étapes, 4 `.ecproj`, aucune écriture dans `/mnt/cache`.
- Durées : STEP1 3 s, STEP2 43 s, STEP3 1 min 40 s, STEP4 2 min 58 s,
  STEP5/6 < 1 s, STEP7 2 s, STEP8 4 s.

### Parité

- STEP2 : CSV identiques sur `cuts`, `short_2997`, `real_244`, `real_245`.
- STEP3 : JSON bruts identiques sur `vox_2spk`, `real_244`, `real_245`.
- STEP4 : JSON identiques à `1e-6` sur `face_pan`, `real_244`, `real_245`.
- STEP5/6 : shadow `162/162` sur 20 projets.
- STEP1 : validation ZIP/RAR/TAR et préservation `.mov`.

### Ingestion

- Import : 911 URL identiques à la base Python.
- Audit de normalisation : 0 changement.
- Webhook réel : 462 enregistrements.
- Téléchargement `276 Camille.zip` : R2 puis repli Dropbox, 183 450 047 octets,
  ZIP valide.
- Sync : 96 archives, 640 hash STEP7, 1,8 Go.

## Prochaines actions

1. **D-024** : diagnostiquer l'écart STEP4 Python sur `153 Camille`
   (4 977 objets contre 11 853). Ne pas utiliser `--prefer-raw` sur ce projet
   tant que la configuration d'origine n'est pas retrouvée.
2. **Reverse proxy** : aucun nginx/caddy/cloudflared n'est installé. La bascule
   publique reste à faire ; voir `docs/SWITCH-PUBLIC.md`.
3. **Surveillance production** : vérifier le moniteur webhook Rust après
   quelques jours d'activité, les téléchargements et la cohérence de
   l'historique.
4. **Documentation** : maintenir `README.md`, `docs/LOT-*.md`,
   `docs/CONTRAT-ENVIRONNEMENT.md` et `parity/DRIFT.md` à jour.
5. **Memory-bank** : mettre à jour `activeContext.md` et ce fichier à chaque
   session significative.

## Politique de rétention

- Ce fichier conserve les tâches depuis octobre 2026.
- Les périodes plus anciennes seront résumées et déplacées vers
  `memory-bank/archives/` si le volume le justifie.
- L'historique Python legacy reste dans
  `/home/kidpixel/workflow_mediapipe/memory-bank/archives/`.
