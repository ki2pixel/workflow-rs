---
name: pipeline-diagnostics
description: Préflight et diagnostic de workflow-rs : config TOML, binaires, modèles, TPU, CUDA, SQLite, ports et verrous.
---

# Diagnostic du pipeline Rust

## Quick start

1. Valider la config : `workflow-cli --config config/config.toml config check`.
2. Valider le serveur : `target/release/workflow-server --config config/config.toml --check`
   avec les secrets exportés.
3. Lancer le préflight complet :
   `set -a; source config/launcher.env; set +a; ./scripts/switch-rust.sh check`.
4. Vérifier les modèles, les bibliothèques TPU, ONNX Runtime, FFmpeg et
   `effectcraft-cli`.
5. Dérouler `resources/env_health_checklist.md` avant un run majeur.

## Procédure complète

### 1. Config TOML

- Lire `config/config.toml`, sections `[server]`, `[paths]`, `[webhook]`,
  `[steps.*]`, `[hardware]`.
- Vérifier que `paths.base`, `projects`, `archives`, `logs`, `downloads` et
  `history_db` sont sous la racine Rust.
- Vérifier `steps.step1.source_dir == paths.downloads` si le webhook est
  activé.
- Vérifier `hardware.lock_path` : par défaut `/tmp/workflow-rs-hw.lock`, jamais
  `/tmp/workflow-hw.lock`.
- Vérifier les chemins absolus machine-specific (`/home/kidpixel/...`,
  `/mnt/cache`, `/mnt/venv_ext4/...`).

### 2. Secrets, ports et isolation

```sh
set -a; source config/launcher.env; set +a
./scripts/switch-rust.sh check
ss -ltnp | grep -E '5003|5004' || true
pgrep -af 'workflow-server|app_new.py|gunicorn' || true
```

- Le serveur refuse `server.port = 5003`.
- Le jeton `WORKFLOW_SERVER_WORKER_TOKEN` ne doit pas être vide ni préfixé
  `dev-`.
- Un seul backend avec le webhook : Python `:5003` ou Rust `:5004`.

### 3. Binaires et build

```sh
cargo build --release -p workflow-server -p workflow-cli
ls -l target/release/workflow-server target/release/workflow-cli
```

- `server.workflow_cli` doit pointer vers un binaire exécutable.
- `server.workdir`, `server.template`, `server.frontend_dir` et
  `server.sound_design_dir` doivent exister.

### 4. Outils externes

```sh
for tool in ffmpeg ffprobe unrar effectcraft-cli; do
  command -v "$tool" || echo "absent: $tool"
done
```

- `ffmpeg`/`ffprobe` : STEP1/2/3/4.
- `unrar` : archives RAR.
- `effectcraft-cli` : STEP8.

### 5. Modèles

```sh
sed -n '1,160p' assets/models/manifest.toml
find assets/models -maxdepth 2 -type f | sort
```

- Vérifier les SHA-256 du manifeste.
- Modèles STEP3 : `yamnet_split_edgetpu_compiled.tflite`,
  `yamnet_split_quantized.tflite`, `yamnet_mel_matrix.npy`,
  `ecapa_tdnn_speaker_float.onnx`.
- Modèles STEP4 : BlazeFace, FaceMesh, blendshapes, EfficientDet, variantes
  CPU de test, paire co-compilée.
- Pour régénérer depuis le legacy : `python3 scripts/import-models.py`
  (outil de copie, pas un fallback pipeline).

### 6. TPU

```sh
ls -l /dev/apex_0
cat /proc/cmdline | grep pcie_aspm=off || true
ls -l vendor/libtensorflowlite_c.so vendor/libedgetpu.so.1
cargo run -p workflow-tpu --bin workflow-tpu-spike -- \
  --model assets/models/step4/blazeface_front_quantized_edgetpu.tflite \
  --edgetpu vendor/libedgetpu.so.1 --iterations 10
```

- Le spike vérifie la signature `edgetpu-custom-op`.
- `steps.step3.require_edgetpu = true` refuse un délégué absent.
- STEP4 utilise un délégué par modèle TPU ; les blendshapes restent sur CPU.

### 7. CUDA / ONNX Runtime

```sh
nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv
ls -l /mnt/venv_ext4/transnet_cv5_env/lib/python3.10/site-packages/onnxruntime/capi/libonnxruntime.so.1.23.2
```

- `steps.step2.device = "cuda"` est le chemin nominal.
- Le mode `--device cpu` est un mode de test explicite.
- Vérifier `cuda_lib_dirs` et `auto_cuda_libs` si le préchargement échoue.
- `use_tf32 = false` et `cudnn_algo_search = true` sont les réglages de
  production.

### 8. SQLite

```sh
sqlite3 runtime/download_history.sqlite3 "PRAGMA integrity_check;"
sqlite3 runtime/download_history.sqlite3 "SELECT COUNT(*) FROM download_history;"
sqlite3 runtime/download_history.sqlite3 "SELECT COUNT(*) FROM download_failures;"
```

- Attendu : `ok` ; l'import du 2026-10-23 contenait 911 URL. Le compte peut
  croître avec le moniteur actif.
- Ne pas modifier la base Python ; `history import` ouvre la source en lecture
  seule.

### 9. Verrous et processus

```sh
ls -l /tmp/workflow-rs-hw.lock /tmp/workflow-hw.lock 2>/dev/null || true
lsof /tmp/workflow-rs-hw.lock 2>/dev/null || true
```

- Le verrou matériel `/tmp/workflow-rs-hw.lock` est pris par STEP2/3/4.
- La sync prend `/tmp/workflow-sync.lock` et refuse de démarrer si `:5003` ou
  `:5004` écoute (sauf `--allow-running`).
- Ne jamais supprimer un verrou tenu par un processus vivant.

### 10. Disque et permissions

- `runtime/` et `runtime/logs/step*` doivent être accessibles en écriture.
- `paths.finalized` (`/mnt/cache`) doit être accessible en lecture/écriture
  pour STEP7, mais jamais utilisé comme espace de test.
- Vérifier l'espace disponible avant STEP1/STEP7/STEP8.

## Résolution rapide

| Symptôme | Action |
| --- | --- |
| `workflow-cli introuvable` | `cargo build --release -p workflow-cli` |
| `configuration invalide` | `workflow-cli config show`, corriger le TOML |
| `isolation refusée` | Vérifier `paths.base`, `history_db`, `projects`, verrou |
| `webhook activé mais download_history vide` | `history import --from ...` ou désactiver `require_history` |
| `source_dir diffère de downloads` | Aligner `steps.step1.source_dir` et `paths.downloads` |
| `libedgetpu` absente | Installer le paquet Coral ou pointer `--edgetpu` |
| `TPU non détecté` | `ls -l /dev/apex_0`, `pcie_aspm=off`, driver gasket |
| `CUDA provider absent` | Vérifier ONNX Runtime CUDA et `cuda_lib_dirs` |
| `port 5004 occupé` | `ss -ltnp`, arrêter l'ancien serveur |
| `409` au lancement | Une étape ou une séquence tourne déjà |

## Références

- `AGENTS.md`
- `.agents/rules/codingstandards.md`
- `docs/CONTRAT-ENVIRONNEMENT.md`
- `docs/SWITCH-PUBLIC.md`
- `docs/RETENTION.md`
- `docs/SYNC-BACKENDS.md`
- `resources/env_health_checklist.md`
