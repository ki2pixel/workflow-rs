# Spike Grounding DINO — résultats bruts

- Généré : 2026-11-09T11:02:01+00:00
- Modèle : `IDEA-Research/grounding-dino-tiny`
- Backend : `{'backend': 'local', 'model_id': 'IDEA-Research/grounding-dino-tiny', 'size': 640, 'device': 'cuda', 'load_s': 3.519, 'calls': 50, 'inference_s': 101.204, 'mean_inference_s': 2.0241, 'peak_vram_gb': 1.838}`

## validation::logo . text . watermark . lower third . banner

Prompt : `logo . text . watermark . lower third . banner` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 100.0% | 100.0% | 100.0% | 0.885 | 50.0% | 100.0% | 0.684 |
| 0.15 | 100.0% | 100.0% | 100.0% | 0.885 | 50.0% | 100.0% | 0.684 |
| 0.20 | 95.0% | 97.5% | 97.5% | 0.885 | 40.0% | 100.0% | 0.684 |
| 0.25 | 92.5% | 92.5% | 92.5% | 0.885 | 40.0% | 90.0% | 0.635 |
| 0.30 | 82.5% | 82.5% | 82.5% | 0.861 | 30.0% | 60.0% | 0.239 |
| 0.35 | 67.5% | 67.5% | 67.5% | 0.766 | 10.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 93.3% | 93.3% | 0.896 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 90.0% | 90.0% | 0.704 | 90.0% | 83.3% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 40.0% |

