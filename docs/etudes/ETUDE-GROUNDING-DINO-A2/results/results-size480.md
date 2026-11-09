# Spike Grounding DINO — résultats bruts

- Généré : 2026-11-09T11:00:02+00:00
- Modèle : `IDEA-Research/grounding-dino-tiny`
- Backend : `{'backend': 'local', 'model_id': 'IDEA-Research/grounding-dino-tiny', 'size': 480, 'device': 'cuda', 'load_s': 3.457, 'calls': 50, 'inference_s': 65.979, 'mean_inference_s': 1.3196, 'peak_vram_gb': 1.474}`

## validation::logo . text . watermark . lower third . banner

Prompt : `logo . text . watermark . lower third . banner` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 100.0% | 100.0% | 100.0% | 0.896 | 60.0% | 100.0% | 0.661 |
| 0.15 | 100.0% | 100.0% | 100.0% | 0.891 | 50.0% | 100.0% | 0.661 |
| 0.20 | 97.5% | 97.5% | 97.5% | 0.882 | 40.0% | 100.0% | 0.659 |
| 0.25 | 90.0% | 92.5% | 92.5% | 0.878 | 40.0% | 90.0% | 0.614 |
| 0.30 | 82.5% | 82.5% | 82.5% | 0.861 | 40.0% | 80.0% | 0.566 |
| 0.35 | 72.5% | 72.5% | 72.5% | 0.812 | 20.0% | 10.0% | 0.014 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 90.0% | 93.3% | 0.904 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 90.0% | 90.0% | 0.714 | 90.0% | 83.3% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 40.0% |

