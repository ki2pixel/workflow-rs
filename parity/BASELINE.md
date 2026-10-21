# Baseline de parité

Généré le 2026-10-21T23:39:20.584767+00:00.

| Cas | Étape | Code | Durée (s) | RSS max (MiB) | GPU étape (MiB) | GPU total (MiB) | Diff | Sorties |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- | ---: |
| `alpha_step1` | STEP1 | 0 | 0.165 | 0.64 | 0 | 406 | identique | 1 |
| `cuts_step2` | STEP2 | 0 | 3.83 | 904.02 | 2226 | 2635 | identique | 1 |
| `face_step4` | STEP4 | 0 | 2.425 | 409.82 | 0 | 407 | identique | 1 |
| `interlaced_step1` | STEP1 | 0 | 2.474 | 1336.68 | 0 | 407 | identique | 1 |
| `real_244_step2` | STEP2 | 0 | 12.151 | 1040.05 | 2224 | 2628 | identique | 1 |
| `real_244_step3` | STEP3 | 0 | 18.025 | 965.27 | 0 | 409 | identique | 1 |
| `real_244_step4` | STEP4 | 0 | 40.516 | 593.26 | 0 | 410 | identique | 1 |
| `real_244_step5` | STEP5 | 0 | 0.567 | 18.33 | 0 | 404 | identique | 1 |
| `real_244_step6` | STEP6 | 0 | 0.466 | 29.27 | 0 | 407 | identique | 1 |
| `real_245_step2` | STEP2 | 0 | 12.105 | 1034.39 | 2224 | 2557 | identique | 1 |
| `real_245_step3` | STEP3 | 0 | 18.025 | 964.41 | 0 | 324 | identique | 1 |
| `real_245_step4` | STEP4 | 0 | 39.572 | 589.51 | 0 | 322 | identique | 1 |
| `real_245_step5` | STEP5 | 0 | 0.521 | 18.45 | 0 | 328 | identique | 1 |
| `real_245_step6` | STEP6 | 0 | 0.521 | 27.04 | 0 | 328 | identique | 1 |
| `real_249_step5` | STEP5 | 0 | 0.691 | 18.21 | 0 | 328 | identique | 1 |
| `real_249_step6` | STEP6 | 0 | 0.517 | 33.07 | 0 | 328 | identique | 1 |
| `real_250_step5` | STEP5 | 0 | 0.381 | 18.57 | 0 | 327 | identique | 1 |
| `real_250_step6` | STEP6 | 0 | 0.319 | 8.72 | 0 | 327 | identique | 1 |
| `short_2997_step1` | STEP1 | 0 | 0.516 | 177.56 | 0 | 407 | identique | 1 |
| `short_2997_step2` | STEP2 | 0 | 3.273 | 864.36 | 2224 | 2633 | identique | 1 |
| `silent_step3` | STEP3 | 0 | 1.068 | 206.64 | 0 | 406 | identique | 0 |
| `vfr_step1` | STEP1 | 0 | 0.616 | 210.79 | 0 | 406 | identique | 1 |
| `vox_step3` | STEP3 | 0 | 5.28 | 904.04 | 0 | 407 | identique | 1 |

## Synthèse par étape

- **STEP1** : 4 cas, durée 0.17–2.47 s, RSS max 1336.7 MiB.
- **STEP2** : 4 cas, durée 3.27–12.15 s, RSS max 1040.0 MiB.
- **STEP3** : 4 cas, durée 1.07–18.02 s, RSS max 965.3 MiB.
- **STEP4** : 3 cas, durée 2.42–40.52 s, RSS max 593.3 MiB.
- **STEP5** : 4 cas, durée 0.38–0.69 s, RSS max 18.6 MiB.
- **STEP6** : 4 cas, durée 0.32–0.52 s, RSS max 33.1 MiB.

## Notes

- `silent_step3` : Sans piste audio, STEP3 sort en 0 après une traceback ffmpeg et ne produit rien.
