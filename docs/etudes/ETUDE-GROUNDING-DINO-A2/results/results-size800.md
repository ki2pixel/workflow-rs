# Spike Grounding DINO — résultats bruts

- Généré : 2026-11-09T11:04:52+00:00
- Modèle : `IDEA-Research/grounding-dino-tiny`
- Backend : `{'backend': 'local', 'model_id': 'IDEA-Research/grounding-dino-tiny', 'size': 800, 'device': 'cuda', 'load_s': 3.489, 'calls': 50, 'inference_s': 152.626, 'mean_inference_s': 3.0525, 'peak_vram_gb': 2.312}`

## validation::logo . text . watermark . lower third . banner

Prompt : `logo . text . watermark . lower third . banner` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 100.0% | 100.0% | 100.0% | 0.897 | 50.0% | 100.0% | 0.713 |
| 0.15 | 97.5% | 100.0% | 100.0% | 0.893 | 50.0% | 100.0% | 0.713 |
| 0.20 | 95.0% | 97.5% | 97.5% | 0.890 | 40.0% | 100.0% | 0.705 |
| 0.25 | 92.5% | 95.0% | 95.0% | 0.882 | 30.0% | 100.0% | 0.659 |
| 0.30 | 87.5% | 87.5% | 87.5% | 0.872 | 30.0% | 70.0% | 0.234 |
| 0.35 | 75.0% | 75.0% | 75.0% | 0.818 | 10.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 93.3% | 96.7% | 0.893 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 90.0% | 90.0% | 0.696 | 100.0% | 100.0% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 30.0% |

