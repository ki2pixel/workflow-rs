---
name: after-effects-json-contract
description: Contrats JSON/CSV consommés par After Effects et le panneau CEP (STEP5/STEP6, ordre des clés, écritures atomiques).
---

# After Effects JSON Contract

> Remplace la skill legacy `after-effects-scripts`. Les scripts JSX et le
> panneau CEP restent en Python (D-022) ; workflow-rs doit produire des
> artefacts strictement compatibles.

## Rôle

Garantir que les sorties Rust consommées par After Effects restent
interchangeables avec la production Python :

- `<stem>.csv` : scènes STEP2 ;
- `<stem>_audio.json` : diarisation STEP3, puis réduction STEP5 ;
- `<stem>_tracking.json` : tracking STEP4, puis réduction STEP5 ;
- `<stem>_ae.json` : pré-traitement STEP6 ;
- manifestes et cuts du pont `workflow-bridge`.

## Contrats

### `_audio.json`

- `fps` et `total_frames` sont écrits **avant** `frames_analysis` (D-003).
- `speaker_stats` est produit par STEP3 ; STEP5 le projette en
  `unique_speakers` et `speaker_frame_counts` (D-007).
- Une seconde réduction conserve les métadonnées.
- Écriture atomique.

### `_tracking.json`

- `fps`, `total_frames`, `frames_analysis`.
- `tracked_objects[]` avec IDs `obj_N`, bbox, centroïde, blendshapes.
- `tracking_analytics`, `temporal_alignment`.
- `speaking_sources.audio` et `.segment` si CSV/audio fournis (D-004).
- Écriture atomique.

### `_ae.json`

- `dataByFrame` filtré sur `face_landmarker` et `object_detector/person`.
- `audioByFrame`, `audioMaxFrame`.
- `segment_recommendations` issues du CSV.
- `is_speaking`, `tier`, `speaker_embeddings`.
- `total_frames`, `tracking_analytics`, `temporal_alignment`,
  `expression_summary`.
- JSON indenté, ordre préservé, écriture atomique.

### CSV

- En-tête `No,Timecode In,Timecode Out,Frame In,Frame Out`.
- Fins de ligne `\r\n`.
- STEP6 lit `<stem>.csv`, jamais `_scenes.csv` (D-001).

## Commandes

```sh
cargo run --release -p workflow-cli -- --config config/config.toml step5 project \
  --work-dir runtime/projets_extraits --keyword Camille

cargo run --release -p workflow-cli -- --config config/config.toml step6 project \
  --work-dir runtime/projets_extraits --keyword Camille
```

Vérifications :

```sh
jq 'keys' runtime/projets_extraits/<projet>/docs/<stem>_ae.json
jq '.total_frames, (.frames_analysis|length)' \
  runtime/projets_extraits/<projet>/docs/<stem>_audio.json
```

## Règles

- Ne pas changer l'ordre des clés.
- Ne pas utiliser `f64::round` pour les valeurs contractuelles.
- Ne pas écrire directement dans le fichier final.
- Ne pas renommer `_ae.json`, `_tracking.json`, `_audio.json`.
- Ne pas supprimer `speaker_embeddings` ni les champs consommés par le JSX.
- Ne pas modifier les scripts AE pour « adapter » une sortie Rust.
- Toute évolution de contrat doit être validée par le shadow STEP5/6 et
  documentée dans `parity/DRIFT.md`.

## Diagnostics

| Symptôme | Piste |
| --- | --- |
| `_ae.json` absent | STEP6 n'a pas trouvé tracking/audio/CSV |
| `segment_recommendations` vide | CSV absent ou mauvais nom |
| `is_speaking` faux partout | Audio absent ou seuil `jawOpen` |
| `dataByFrame` vide | Aucun visage/personne retenu |
| Clés dans le désordre | `preserve_order` désactivé |
| JSON tronqué | Écriture non atomique ou disque plein |
| Deuxième réduction perd `fps` | En-tête écrit après `frames_analysis` |
| JSX ne lit pas le fichier | Nom ou chemin non conforme |

## Références

- `docs/pipeline/LOT-1-STEP5.md`
- `docs/pipeline/LOT-1-STEP6.md`
- `docs/pipeline/LOT-1-BRIDGE.md`
- `parity/DRIFT.md` (D-001, D-002, D-003, D-004, D-007)
- `.agents/skills/after-effects-bridge/SKILL.md`
- `crates/workflow-steps/src/step5/`
- `crates/workflow-steps/src/step6.rs`
