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

Les originaux restent la référence de production. Toute modification de modèle
doit être répercutée dans `manifest.toml` et dans le contrat.
