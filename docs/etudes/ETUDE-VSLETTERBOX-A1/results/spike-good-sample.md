# Matrice de confusion — spike A1 natif (gray)

| A1 \ natif | apply | review | noop |
| --- | ---: | ---: | ---: |
| apply | 25 | 26 | 14 |
| review | 0 | 12 | 104 |
| noop | 0 | 1 | 19 |

## Taux

- Faux `apply` natifs sur A1 review/noop : **0/123** (0.0 %).
- Faux négatifs natifs sur les 56 applies clairs : **32/56** (57.1 %).
- Faux vetos résolus : **0/13** ; asymétriques **0/7** ; couverture **0/6**.
- Accord `cropdetect=24:16:0` : **25/80** ; `24:2:0` : **25/78**.

## Détail

| Sous-ensemble | n | apply | review | noop |
| --- | ---: | ---: | ---: | ---: |
| apply accepted | 11 | 6 | 2 | 3 |
| apply adjusted | 45 | 18 | 21 | 6 |
| apply reverted | 9 | 1 | 3 | 5 |
| review veto | 92 | 0 | 2 | 90 |
| review resolved | 13 | 0 | 5 | 8 |
| review asym résolues | 7 | 0 | 2 | 5 |
| review couverture résolues | 6 | 0 | 3 | 3 |
| review autres | 11 | 0 | 5 | 6 |
| noop | 20 | 0 | 1 | 19 |

## Pixel (applies natifs vs cropdetect 2 px)

- Natif : {'n': 25, 'mae_top': 0.84, 'mae_bottom': 0.68, 'max': 2, 'le_1_px': 24, 'le_2_px': 25, 'le_4_px': 25}
- cropdetect 16 px : {'n': 63, 'mae_top': 2.95, 'mae_bottom': 2.06, 'max': 8, 'le_1_px': 1, 'le_2_px': 37, 'le_4_px': 59}
- Hybride projeté (natif si apply, sinon cd16) : {'n': 63, 'mae_top': 1.98, 'mae_bottom': 1.54, 'max': 8, 'le_1_px': 25, 'le_2_px': 53, 'le_4_px': 59}

## Coût

- Natif médian : 450.7 ms ; ratio médian vs cropdetect 16 px : 1.12×.
- Applies raffinés par le natif : 25 ; fallback cropdetect : 40.
