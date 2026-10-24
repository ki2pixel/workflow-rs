---
name: coral-tpu-hybrid-algorithms
description: Algorithmes CPU Rust qui complètent l'inférence Coral (médiane, hangover, AHC, One-Euro, arrondis) et leur parité.
---

# Coral TPU Hybrid Algorithms

## Rôle

Le Coral ne fait que l'inférence INT8. Les calculs qui compensent la
quantification et les limites de l'ASIC sont exécutés en Rust sur le CPU :

- **STEP3** : frontend log-mel, médiane, seuil VAD, hangover, AHC, distance
  cosinus, réassignation des locuteurs mineurs.
- **STEP4** : One-Euro 52D, suivi centroïde, détection de parole.
- **STEP5** : analytics, moyennes, spreads, arrondis.
- **STEP6** : recommandations de segments, arrondis.
- **`workflow-schema`** : `round2`, `round4`, `round_to` compatibles Python.

## Modules de référence

| Sujet | Fichier |
| --- | --- |
| Mel / VAD | `crates/workflow-steps/src/step3/mel.rs`, `vad.rs` |
| ECAPA / AHC | `crates/workflow-steps/src/step3/ecapa.rs`, `cluster.rs` |
| Filtre One-Euro | `crates/workflow-steps/src/step4/filter.rs` |
| Parole | `crates/workflow-steps/src/step4/speaking.rs` |
| Suivi | `crates/workflow-steps/src/step4/tracking.rs` |
| Analytics | `crates/workflow-steps/src/step5/analytics.rs` |
| Arrondis | `crates/workflow-schema/src/round.rs` |

## Checklist de préparation

- [ ] Le modèle TPU et la version du frontend sont connus.
- [ ] Les fixtures de parité sont disponibles.
- [ ] Les seuils de config sont cohérents (`vad_threshold`,
      `clustering_threshold`, `hangover_sec`, `jaw_open_threshold`).
- [ ] Les tests ciblés passent.
- [ ] Aucun calcul lourd n'est déporté sur le thread TPU.

## Commandes

```sh
cargo test -p workflow-steps step3 --lib
cargo test -p workflow-steps step4 --lib
cargo test -p workflow-steps step5 --lib
cargo test -p workflow-schema --lib
```

Parité :

```sh
python3 scripts/shadow-step56.py --root /mnt/cache --limit 10
python3 scripts/validate-lot5.py
```

## Points de vigilance

### STEP3

- Fenêtres 15 360 échantillons, hop 7 680 (0,48 s).
- Frontend 400/160, FFT 512, 64 mel.
- Médiane 5, seuil strict 0,20, hangover 1,0 s.
- AHC moyenne, cosinus, seuil 0,32.
- Réassignation des clusters < 7 s.
- `embedding_stride = 1` par défaut.

### STEP4

- One-Euro float64 sur 52 dimensions.
- Mise à jour uniquement sur les frames avec visage.
- Suivi centroïde, seuil 100 px, oubli après 10 frames.
- Tri par `centroid_x` comme Python.
- Parole audio/visuelle et segments CSV.

### Arrondis

- Pour toute valeur contractuelle, utiliser `round2`, `round4` ou `round_to`.
- Jamais `f64::round` pour ces valeurs ; les calculs internes peuvent utiliser
  `round_ties_even` lorsque la parité Python le justifie.
- Tester les demi-valeurs (`0.125`, `2.675`, `2919.125`).

## Incidents

| Symptôme | Piste |
| --- | --- |
| Jitter extrême STEP4 | Vérifier One-Euro, les frames sans visage, le filtre |
| Clustering audio instable | Vérifier AHC, cosinus, seuil, embeddings normalisés |
| VAD trop bavarde | Vérifier médiane, seuil, hangover, frontend mel |
| Écart de parité numérique | Vérifier l'ordre des opérations et les arrondis |
| CPU saturé | Réduire le parallélisme, vérifier les allocations par frame |
| TPU lent | Vérifier que les post-traitements ne bloquent pas l'inférence |

## Séparation des responsabilités

- L'inférence TPU reste dans `workflow-tpu` et les modules STEP3/STEP4.
- Les algorithmes CPU ne doivent pas ouvrir de session TFLite.
- Un calcul lourd ne doit pas être exécuté dans le callback de progression ou
  dans le writer JSON.
