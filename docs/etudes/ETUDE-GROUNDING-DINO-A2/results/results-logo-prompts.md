# Spike Grounding DINO — résultats bruts

- Généré : 2026-11-09T11:11:22+00:00
- Modèle : `IDEA-Research/grounding-dino-tiny`
- Backend : `{'backend': 'local', 'model_id': 'IDEA-Research/grounding-dino-tiny', 'size': 480, 'device': 'cuda', 'load_s': 3.636, 'calls': 250, 'inference_s': 316.277, 'mean_inference_s': 1.2651, 'peak_vram_gb': 1.469}`

## prompt::logo . watermark

Prompt : `logo . watermark` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 100.0% | 100.0% | 100.0% | 0.836 | 90.0% | 100.0% | 0.628 |
| 0.15 | 100.0% | 100.0% | 100.0% | 0.836 | 50.0% | 50.0% | 0.628 |
| 0.20 | 95.0% | 95.0% | 95.0% | 0.831 | 50.0% | 30.0% | 0.289 |
| 0.25 | 77.5% | 77.5% | 77.5% | 0.805 | 50.0% | 20.0% | 0.055 |
| 0.30 | 65.0% | 67.5% | 67.5% | 0.677 | 50.0% | 0.0% | 0.000 |
| 0.35 | 52.5% | 55.0% | 55.0% | 0.663 | 30.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 76.7% | 76.7% | 0.831 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 80.0% | 80.0% | 0.669 | 20.0% | 33.3% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 50.0% |

## prompt::brand logo . watermark

Prompt : `brand logo . watermark` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 100.0% | 100.0% | 100.0% | 0.857 | 90.0% | 80.0% | 0.615 |
| 0.15 | 100.0% | 100.0% | 100.0% | 0.855 | 60.0% | 50.0% | 0.615 |
| 0.20 | 100.0% | 100.0% | 100.0% | 0.855 | 50.0% | 20.0% | 0.054 |
| 0.25 | 90.0% | 90.0% | 90.0% | 0.849 | 30.0% | 20.0% | 0.012 |
| 0.30 | 80.0% | 80.0% | 80.0% | 0.798 | 20.0% | 0.0% | 0.000 |
| 0.35 | 67.5% | 67.5% | 67.5% | 0.762 | 10.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 90.0% | 90.0% | 0.855 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 90.0% | 90.0% | 0.688 | 20.0% | 33.3% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 30.0% |

## prompt::logo . symbol . watermark

Prompt : `logo . symbol . watermark` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 100.0% | 100.0% | 100.0% | 0.848 | 70.0% | 60.0% | 0.604 |
| 0.15 | 100.0% | 100.0% | 100.0% | 0.845 | 50.0% | 40.0% | 0.604 |
| 0.20 | 97.5% | 97.5% | 97.5% | 0.839 | 50.0% | 30.0% | 0.362 |
| 0.25 | 82.5% | 82.5% | 82.5% | 0.832 | 50.0% | 10.0% | 0.000 |
| 0.30 | 72.5% | 72.5% | 72.5% | 0.780 | 50.0% | 0.0% | 0.000 |
| 0.35 | 55.0% | 55.0% | 57.5% | 0.669 | 30.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 83.3% | 83.3% | 0.839 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 80.0% | 80.0% | 0.675 | 10.0% | 16.7% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 50.0% |

## prompt::channel logo . watermark

Prompt : `channel logo . watermark` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 100.0% | 100.0% | 100.0% | 0.852 | 90.0% | 100.0% | 0.603 |
| 0.15 | 100.0% | 100.0% | 100.0% | 0.852 | 60.0% | 70.0% | 0.603 |
| 0.20 | 100.0% | 100.0% | 100.0% | 0.852 | 50.0% | 50.0% | 0.522 |
| 0.25 | 90.0% | 90.0% | 90.0% | 0.847 | 50.0% | 20.0% | 0.000 |
| 0.30 | 75.0% | 75.0% | 75.0% | 0.798 | 30.0% | 20.0% | 0.000 |
| 0.35 | 67.5% | 67.5% | 67.5% | 0.764 | 20.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 90.0% | 90.0% | 0.852 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 90.0% | 90.0% | 0.685 | 20.0% | 33.3% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 50.0% |

## prompt::text . lower third . banner

Prompt : `text . lower third . banner` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 97.5% | 97.5% | 100.0% | 0.819 | 0.0% | 100.0% | 0.717 |
| 0.15 | 87.5% | 92.5% | 97.5% | 0.819 | 0.0% | 100.0% | 0.717 |
| 0.20 | 77.5% | 85.0% | 90.0% | 0.819 | 0.0% | 100.0% | 0.717 |
| 0.25 | 62.5% | 70.0% | 75.0% | 0.788 | 0.0% | 90.0% | 0.612 |
| 0.30 | 57.5% | 62.5% | 65.0% | 0.719 | 0.0% | 70.0% | 0.324 |
| 0.35 | 45.0% | 50.0% | 50.0% | 0.356 | 0.0% | 50.0% | 0.035 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 73.3% | 90.0% | 0.819 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 30.0% | 30.0% | 0.000 | 90.0% | 66.7% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |

