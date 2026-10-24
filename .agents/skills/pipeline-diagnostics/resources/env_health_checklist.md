# Checklist santé — workflow-rs

> À dérouler avant un run majeur ou après un changement de configuration, de
> modèle, de bibliothèque ou de matériel.

## 1. Config et secrets

```sh
cargo run -p workflow-cli -- --config config/config.toml config check
cargo run -p workflow-cli -- --config config/config.toml config show
set -a; source config/launcher.env; set +a
./scripts/switch-rust.sh check
```

- [ ] `config check` passe.
- [ ] `config show` masque les secrets.
- [ ] `config/launcher.env` existe et est en `chmod 600`.
- [ ] `server.port = 5004`, pas 5003.
- [ ] `hardware.lock_path` est `/tmp/workflow-rs-hw.lock`.

## 2. Répertoires

```sh
ls -ld runtime runtime/projets_extraits runtime/archives runtime/logs runtime/downloads
ls -ld /mnt/cache
```

- [ ] Tous les répertoires existent.
- [ ] `runtime/` est accessible en écriture.
- [ ] `/mnt/cache` est accessible pour STEP7/STEP8.
- [ ] Aucun test n'écrit dans `/mnt/cache`.

## 3. Binaires

```sh
ls -l target/release/workflow-server target/release/workflow-cli
```

- [ ] Les deux binaires sont en mode release et exécutables.
- [ ] `server.workflow_cli` pointe vers le bon chemin.

## 4. Outils

```sh
for tool in ffmpeg ffprobe unrar effectcraft-cli; do
  printf '%-16s' "$tool"; command -v "$tool" || echo absent
done
```

- [ ] `ffmpeg`/`ffprobe` disponibles.
- [ ] `unrar` disponible si des RAR sont attendus.
- [ ] `effectcraft-cli` disponible pour STEP8.

## 5. Modèles

```sh
find assets/models -maxdepth 3 -type f | sort
sha256sum assets/models/step3/* assets/models/step4/* 2>/dev/null | head
```

- [ ] Les fichiers du manifeste sont présents.
- [ ] Les SHA-256 correspondent à `assets/models/manifest.toml`.
- [ ] La paire co-compilée STEP4 est présente si `cocompiled = true`.
- [ ] Les modèles CPU de test sont présents pour `--device cpu`.

## 6. TPU

```sh
ls -l /dev/apex_0
cat /proc/cmdline | grep pcie_aspm=off || true
ls -l vendor/libtensorflowlite_c.so vendor/libedgetpu.so.1
cargo run -p workflow-tpu --bin workflow-tpu-spike -- \
  --model assets/models/step4/blazeface_front_quantized_edgetpu.tflite \
  --edgetpu vendor/libedgetpu.so.1 --iterations 10
```

- [ ] `/dev/apex_0` présent.
- [ ] `pcie_aspm=off` présent.
- [ ] Les deux bibliothèques sont lisibles.
- [ ] Le spike détecte `edgetpu-custom-op` et produit une sortie stable.

## 7. GPU / ONNX Runtime

```sh
nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv
ls -l /mnt/venv_ext4/transnet_cv5_env/lib/python3.10/site-packages/onnxruntime/capi/libonnxruntime.so.1.23.2
```

- [ ] GPU visible.
- [ ] ONNX Runtime CUDA présent.
- [ ] `cuda_lib_dirs` correct si le préchargement automatique échoue.

## 8. SQLite

```sh
sqlite3 runtime/download_history.sqlite3 "PRAGMA integrity_check;"
sqlite3 runtime/download_history.sqlite3 "SELECT COUNT(*) FROM download_history;"
sqlite3 runtime/download_history.sqlite3 "SELECT COUNT(*) FROM download_failures;"
```

- [ ] `integrity_check` retourne `ok`.
- [ ] L'historique contient les URL importées.
- [ ] Les échecs sont cohérents.

## 9. Serveur et ports

```sh
ss -ltnp | grep -E '5003|5004' || true
pgrep -af 'workflow-server|app_new.py|gunicorn' || true
curl -fs -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" \
  http://127.0.0.1:5004/api/ping || true
```

- [ ] Un seul backend avec le webhook.
- [ ] `:5004` répond si Rust est démarré.
- [ ] `:5003` est arrêté si le moniteur Rust est actif.
- [ ] Aucun processus STEP2/3/4 orphelin.

## 10. Checklist finale

- [ ] Config validée.
- [ ] Binaires et outils présents.
- [ ] Modèles et SHA-256 valides.
- [ ] TPU/CUDA prêts.
- [ ] SQLite sain.
- [ ] Ports et verrous cohérents.
- [ ] Sorties de test sous `/tmp/opencode`, pas `/mnt/cache`.
- [ ] Un plan de rollback est connu pour toute opération destructive.
