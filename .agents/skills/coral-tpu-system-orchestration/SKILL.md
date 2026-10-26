---
name: coral-tpu-system-orchestration
description: Pont matériel Coral Edge TPU dans workflow-rs : FFI dynamique, /dev/apex_0, verrou, sérialisation et diagnostics.
---

# Coral TPU System Orchestration

## Rôle

Maintenir l'intégration matérielle du Coral M.2 PCIe et son orchestration Rust :

- **Macro** : le serveur sérialise les étapes ; une seule étape matérielle à la
  fois.
- **Verrou** : `HardwareLock` sur `/tmp/workflow-rs-hw.lock` pour STEP1
  (normalisation vidéo) et STEP2/3/4. La sync utilise son propre verrou
  `/tmp/workflow-sync.lock` et refuse de démarrer si un serveur écoute.
- **Micro** : `workflow-tpu::Session` charge `libtensorflowlite_c.so` et
  `libedgetpu.so.1`, vérifie la signature `edgetpu-custom-op` et sérialise
  l'accès aux tenseurs.
- **STEP3** : une session YAMNet réutilisée pour toutes les fenêtres ; le
  pipeline est séquentiel, donc aucun accès concurrent.
- **STEP4** : un délégué Edge TPU par modèle TPU ; les blendshapes restent sur
  CPU.

## Checklist de préparation

- [ ] `cat /proc/cmdline | grep pcie_aspm=off`.
- [ ] `ls -l /dev/apex_0`.
- [ ] `ls -l vendor/libtensorflowlite_c.so vendor/libedgetpu.so.1`.
- [ ] `steps.step3.require_edgetpu = true` en production.
- [ ] `hardware.lock_path = "/tmp/workflow-rs-hw.lock"`.
- [ ] Aucun processus Python ne tient `/tmp/workflow-hw.lock`.
- [ ] Aucun autre workflow Rust ne tourne.

## Commandes

```sh
ls -l /dev/apex_0
cat /proc/cmdline | grep pcie_aspm=off || true
dmesg | grep -i -E 'apex|gasket' | tail -20

cargo run -p workflow-tpu --bin workflow-tpu-spike -- \
  --model assets/models/step4/blazeface_front_quantized_edgetpu.tflite \
  --edgetpu vendor/libedgetpu.so.1 --iterations 1000
```

Vérifier le verrou :

```sh
ls -l /tmp/workflow-rs-hw.lock
lsof /tmp/workflow-rs-hw.lock 2>/dev/null || true
```

Vérifier la config :

```sh
cargo run -p workflow-cli -- --config config/config.toml config show | \
  jq '.hardware, .steps.step3.device, .steps.step4.device'
```

## Incidents

| Symptôme | Piste |
| --- | --- |
| TPU non détecté | Driver gasket, `/dev/apex_0`, ASPM, AER |
| `dlopen` échoue | Chemin de `libedgetpu.so.1` ou `libtensorflowlite_c.so` |
| `edgetpu-custom-op` absent | Modèle non compilé pour l'Edge TPU |
| OOM TPU | Modèle trop gros ou sessions concurrentes ; vérifier la SRAM |
| Verrou refusé | STEP1 (normalisation), STEP2/3/4 ou sync déjà en cours |
| Deux étapes en parallèle | Le serveur doit renvoyer 409 ; vérifier l'état |
| Résultats non déterministes | Accès aux tenseurs hors verrou |
| `pcie_aspm` actif | Ajouter `pcie_aspm=off` au kernel et redémarrer |

## Règles

- Ne jamais supprimer un verrou tenu par un processus vivant.
- Ne jamais partager un interpréteur TFLite entre threads sans verrou couvrant
  écriture, `invoke()` et lecture.
- Ne pas lancer deux étapes matérielles en parallèle, même sur des TPU
  différents.
- Le verrou Python `/tmp/workflow-hw.lock` est interdit côté Rust.
- Ne pas modifier `/dev/apex_0`, les permissions ou le driver sans plan de
  rollback.

## Références

- `crates/workflow-tpu/src/session.rs`
- `crates/workflow-core/src/lock.rs`
- `docs/pipeline/LOT-3-STEP4.md`
- `docs/pipeline/LOT-4-STEP3.md`
- `docs/contracts/CONTRAT-ENVIRONNEMENT.md`
- `.agents/skills/coral-tpu-model-compiler/SKILL.md`
