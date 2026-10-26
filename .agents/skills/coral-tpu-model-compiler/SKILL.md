---
name: coral-tpu-model-compiler
description: Préparer, importer et vérifier les modèles TFLite INT8/Edge TPU utilisés par workflow-rs (manifeste, co-compilation, spike).
---

# Coral TPU Model Compiler

## Rôle

Le dépôt Rust ne compile pas les modèles à la volée. Il consomme des modèles
déjà quantifiés et compilés, décrits par `assets/models/manifest.toml`.

- `scripts/import-models.py` copie les modèles de production depuis le dépôt
  Python et met à jour le manifeste.
- `edgetpu_compiler` reste un outil externe, utilisé hors du pipeline Rust.
- `workflow-tpu-spike` vérifie la signature `edgetpu-custom-op`, charge le
  modèle et exécute quelques inférences.
- Les modèles binaires sont gitignorés ; seuls le manifeste et le README sont
  versionnés.

## Checklist de préparation

- [ ] Le modèle cible est identifié (STEP3 ou STEP4).
- [ ] Le manifeste contient son rôle, sa taille et son SHA-256.
- [ ] Les entrées/sorties sont en `uint8`/`int8` pour l'Edge TPU.
- [ ] Le modèle contient `edgetpu-custom-op`.
- [ ] La paire co-compilée STEP4 est cohérente.
- [ ] Les variantes CPU de test sont présentes.
- [ ] Aucune recompilation spéculative n'est lancée.

## Commandes

```sh
python3 scripts/import-models.py
sed -n '1,180p' assets/models/manifest.toml
sha256sum assets/models/step3/* assets/models/step4/* 2>/dev/null
```

Vérification TPU :

```sh
cargo run -p workflow-tpu --bin workflow-tpu-spike -- \
  --model assets/models/step4/blazeface_front_quantized_edgetpu.tflite \
  --edgetpu vendor/libedgetpu.so.1 --iterations 10
```

Signature :

```sh
strings assets/models/step3/yamnet_split_edgetpu_compiled.tflite | \
  grep -c edgetpu-custom-op
```

Compilation externe (hors pipeline) :

```sh
edgetpu_compiler -s model_quantized.tflite
```

## Incidents

| Symptôme | Piste |
| --- | --- |
| Opérations massives sur CPU | Modèle mal compilé ; vérifier le rapport `edgetpu_compiler` |
| Perte de précision | Vérifier la quantification INT8, le jeu de calibration |
| `edgetpu-custom-op` absent | Le modèle n'est pas compilé pour l'Edge TPU |
| Modèle absent | `python3 scripts/import-models.py` |
| SHA-256 différent | Restaurer le fichier du manifeste ou mettre à jour le manifeste |
| Entrée/sortie float | Le modèle n'est pas compatible Edge TPU |
| Co-compilation incohérente | Vérifier la paire BlazeFace/FaceMesh |
| Spike OK mais STEP échoue | Vérifier le chemin de la bibliothèque et le device |

## Règles

- Ne pas modifier un modèle binaire sans mettre à jour le manifeste.
- Ne pas remplacer un modèle par une version non quantifiée pour « faire
  passer » un test.
- Ne pas ajouter de fallback CPU silencieux : le mode CPU est explicite.
- Ne pas compiler le frontend log-mel ni ECAPA pour l'Edge TPU : ces calculs
  restent sur CPU.
- Consigner tout changement de modèle dans `memory-bank/decisionLog.md` et
  `parity/DRIFT.md` si la parité est affectée.

## Références

- `assets/models/README.md`
- `assets/models/manifest.toml`
- `docs/pipeline/LOT-3-STEP4.md`
- `docs/pipeline/LOT-4-STEP3.md`
- `.agents/skills/coral-tpu-system-orchestration/SKILL.md`
