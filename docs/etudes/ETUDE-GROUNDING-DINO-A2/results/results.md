# Spike Grounding DINO — résultats bruts

- Généré : 2026-11-09T10:56:06+00:00
- Modèle : `IDEA-Research/grounding-dino-tiny`
- Backend : `{'backend': 'local', 'model_id': 'IDEA-Research/grounding-dino-tiny', 'size': 480, 'device': 'cuda', 'load_s': 3.475, 'calls': 592, 'inference_s': 757.054, 'mean_inference_s': 1.2788, 'peak_vram_gb': 1.471}`

## prompt::logo

Prompt : `logo` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 12.5% | 20.0% | 22.5% | 0.000 | 70.0% | 0.0% | 0.035 |
| 0.15 | 5.0% | 10.0% | 10.0% | 0.000 | 30.0% | 0.0% | 0.000 |
| 0.20 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.25 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.30 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.35 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |

## prompt::text

Prompt : `text` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.15 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.20 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.25 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.30 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.35 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |

## prompt::logo . text

Prompt : `logo . text` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 100.0% | 100.0% | 100.0% | 0.829 | 80.0% | 90.0% | 0.629 |
| 0.15 | 100.0% | 100.0% | 100.0% | 0.829 | 50.0% | 50.0% | 0.629 |
| 0.20 | 90.0% | 90.0% | 90.0% | 0.825 | 50.0% | 30.0% | 0.291 |
| 0.25 | 75.0% | 75.0% | 75.0% | 0.770 | 50.0% | 10.0% | 0.000 |
| 0.30 | 62.5% | 65.0% | 65.0% | 0.672 | 50.0% | 0.0% | 0.000 |
| 0.35 | 47.5% | 50.0% | 52.5% | 0.384 | 30.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 73.3% | 73.3% | 0.825 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 80.0% | 80.0% | 0.669 | 10.0% | 16.7% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 50.0% |

## prompt::lower third

Prompt : `lower third` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 10.0% | 0.014 |
| 0.15 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.20 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.25 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.30 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.35 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |

## prompt::television logo

Prompt : `television logo` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 20.0% | 25.0% | 27.5% | 0.000 | 80.0% | 10.0% | 0.046 |
| 0.15 | 2.5% | 7.5% | 7.5% | 0.000 | 20.0% | 0.0% | 0.035 |
| 0.20 | 0.0% | 2.5% | 2.5% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.25 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.30 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.35 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |

## prompt::banner

Prompt : `banner` — 50 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.014 |
| 0.15 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.20 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.25 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.30 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |
| 0.35 | 0.0% | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 30 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |
| negative | 10 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 0.0% |

## prompt::logo . text . watermark . lower third . banner

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

## validation::logo . text . watermark . lower third . banner

Prompt : `logo . text . watermark . lower third . banner` — 242 frames

| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.10 | 99.5% | 99.5% | 99.5% | 0.894 | 51.7% | 100.0% | 0.657 |
| 0.15 | 99.5% | 99.5% | 99.5% | 0.889 | 33.3% | 100.0% | 0.657 |
| 0.20 | 97.2% | 97.8% | 97.2% | 0.883 | 25.0% | 98.3% | 0.655 |
| 0.25 | 91.8% | 92.3% | 92.3% | 0.876 | 20.0% | 91.7% | 0.638 |
| 0.30 | 84.6% | 84.6% | 84.6% | 0.854 | 15.0% | 68.3% | 0.276 |
| 0.35 | 75.8% | 75.8% | 75.8% | 0.790 | 8.3% | 16.7% | 0.000 |

Détail par source au seuil 0,25 :

| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| reference_logo | 122 | 91.8% | 92.6% | 0.893 | 0.0% | 0.0% | 0.0% |
| a2_lower_third | 60 | 91.7% | 91.7% | 0.715 | 91.7% | 77.8% | 0.0% |
| negative | 60 | 0.0% | 0.0% | 0.000 | 0.0% | 0.0% | 20.0% |

### Persistance temporelle (seuil 0,20)

| clip | frames | persistance logo | persistance texte | logo 6f | texte 6f |
| --- | --- | --- | --- | --- | --- |
| 40_66m_abhartach | 12 | 100.0% | 66.7% | 100.0% | 100.0% |
| 40_66m_citrouilles | 12 | 100.0% | 83.3% | 100.0% | 100.0% |
| 40_66m_footballeur | 12 | 100.0% | 50.0% | 100.0% | 100.0% |
| 54_ujud_hotel | 12 | 100.0% | 100.0% | 100.0% | 100.0% |
| 58_ujud_crousty | 12 | 91.7% | 91.7% | 100.0% | 100.0% |

