# Modèles

Les modèles sont **copiés** depuis `/home/kidpixel/workflow_mediapipe` pour que
le workspace Rust soit autonome à l'exécution. Les binaires ne sont pas
versionnés ; `manifest.toml` contient leur rôle, leur taille et leur SHA-256.

Pour les recopier ou vérifier les empreintes :

```bash
python3 scripts/import-models.py
```

| Rôle | Fichier |
| --- | --- |
| STEP2 | `step2/transnetv2.onnx` |
| STEP3 | `step3/yamnet_split_edgetpu_compiled.tflite`, `step3/yamnet_split_quantized.tflite` |
| STEP3 | `step3/ecapa_tdnn_speaker_float.onnx` + `.onnx.data` |
| STEP4 | `step4/blazeface_front_quantized_edgetpu.tflite`, `step4/facemesh_quantized_edgetpu.tflite` |
| STEP4 | `step4/face_blendshapes.tflite` |
| STEP4 | `step4/cocompiled/*.tflite` |
| STEP4 | `step4/efficientdet_lite2_int8_mediapipe.tflite`, `step4/detect_edgetpu.tflite` |
| STEP4 CPU (tests) | `step4/blazeface_front_quantized.tflite`, `step4/facemesh_quantized.tflite`, `step4/detect.tflite` |
| QA (optionnel) | `qa/dinov2_vits14_quantized.onnx` + `qa/dinov2_logo_probe.json` |

## QA visuel optionnel

Le vérificateur DINOv2 n'est pas copié par `scripts/import-models.py` : il ne
vient pas de l'application Python. Avec `--logo-visual`, `workflow-cli qa run`
cherche `dinov2_vits14_quantized.onnx` et `dinov2_logo_probe.json` dans le
répertoire `--models`. Le probe est versionné dans `qa/` ; le binaire se
télécharge à part (Apache-2.0) :

```bash
curl -L --fail -o /chemin/models/dinov2_vits14_quantized.onnx \
  https://huggingface.co/onnx-community/dinov2-small/resolve/main/onnx/model_quantized.onnx
cp assets/models/qa/dinov2_logo_probe.json /chemin/models/
sha256sum /chemin/models/dinov2_vits14_quantized.onnx
# attendu : c179f8f7f592449c4c1bca4cd124a7538021428c5ffb89afde9503935b197efb
```

Le manifeste `qa/manifest.toml` fige la taille, le SHA-256 et le point de
fonctionnement. Sans ces fichiers, `logo_visual` est `not_applicable` ; le
contrôle texte reste actif.

Les originaux restent la référence de production. Toute modification de modèle
doit être répercutée dans `manifest.toml` et dans le contrat.
