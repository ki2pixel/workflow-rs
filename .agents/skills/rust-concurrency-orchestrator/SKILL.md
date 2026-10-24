---
name: rust-concurrency-orchestrator
description: Patrons de concurrence Rust de workflow-rs (tokio, rayon, canaux bornés, verrous, sérialisation serveur, SIGTERM).
---

# Rust Concurrency Orchestrator

> Remplace la skill legacy `cv5-multiprocessing-orchestrator`. workflow-rs
> n'utilise ni OpenCV 5.0 DNN ni multiprocessing Python.

## Rôle

Choisir et vérifier le bon modèle de concurrence :

| Besoin | Outil |
| --- | --- |
| Décoder une vidéo / lancer un binaire | `tokio::process` (STEP2, serveur) ou `std::process` (STEP1/3/4/8) |
| Travail CPU lourd dans un contexte async | `tokio::task::spawn_blocking` |
| Parallélisme de fichiers STEP1 | `rayon` |
| Flux producteur/consommateur STEP2 | `tokio::sync::mpsc` borné (capacité 2) |
| Flux frames STEP4 | `std::sync::mpsc::sync_channel(3)` |
| Téléchargements bornés | `tokio::sync::Semaphore` |
| Moteur partagé | `Arc<Mutex<...>>` |
| Accès matériel exclusif | `HardwareLock` |
| API concurrente | état `Arc<AppState>`, sérialisation 409 |
| Arrêt propre | `kill_on_drop`, SIGTERM, `ctrlc` |

## Patrons par étape

### STEP1

- `rayon::ThreadPoolBuilder` pour normaliser les vidéos.
- Pool borné par `steps.step1.gpu_workers`.
- Aucune écriture concurrente sur un même fichier.

### STEP2

- FFmpeg en `tokio::process`.
- `WindowStream` + canal borné de 2 lots.
- `Arc<Mutex<TransNetEngine>>` pour l'inférence.
- Backend CPU explicite, pas de fallback.

### STEP3

- Une session YAMNet réutilisée séquentiellement.
- Ne pas la partager entre threads sans un verrou couvrant l'écriture du
  tenseur, `invoke()` et la lecture.
- Traitement séquentiel des vidéos.
- ECAPA sur CPU, batch 32.

### STEP4

- Thread producteur FFmpeg borné à 3 trames.
- Cascade TPU chargée une fois pour toutes les vidéos.
- Un délégué TPU par modèle.
- Écriture JSON streaming vers un temporaire.

### STEP8

- `effectcraft-cli` en sous-processus, avec son propre groupe de processus.
- `ctrlc`/SIGTERM : terminer le groupe enfant avant de sortir.
- Timeout par watchdog : SIGTERM, puis SIGKILL après le délai de grâce.

### Ingestion

- `Semaphore` pour `download.max_concurrent`.
- `tokio::spawn` par URL, mais concurrence bornée.
- SQLite protégé par `Arc<Mutex<Connection>>`.
- `busy_timeout` de 30 s.

### Serveur

- Une seule étape à la fois.
- Un deuxième lancement renvoie 409.
- Une séquence s'arrête au premier échec.
- `stop`/`cancel` passent par `stopping`, envoient SIGTERM puis SIGKILL et
  conservent le créneau jusqu'à la mort du sous-processus ; un `finish_step`
  périmé est ignoré (`RunId`).

## Checklist

- [ ] Aucun canal non borné sur un flux de frames ou de fenêtres.
- [ ] Aucun `Mutex` tenu à travers un `await`.
- [ ] Aucun `spawn_blocking` oublié pour un calcul CPU lourd.
- [ ] Aucun accès TFLite hors verrou.
- [ ] Aucun lancement parallèle de STEP2/3/4.
- [ ] Les sous-processus sont tués en cas de timeout/annulation.
- [ ] Les erreurs remontent, pas de panique silencieuse.
- [ ] Le serveur sérialise les étapes.

## Diagnostics

```sh
rg -n 'tokio::spawn|spawn_blocking|rayon|mpsc|sync_channel|Semaphore|Mutex' crates
cargo test --workspace
RUST_LOG=debug cargo run --release -p workflow-server -- --config config/config.toml
```

- `perf stat`, `perf top`, `strace -f` pour les blocages.
- `RUST_BACKTRACE=1` pour les paniques.
- `lsof /tmp/workflow-rs-hw.lock` pour le verrou.

## Incidents

| Symptôme | Piste |
| --- | --- |
| Deadlock | Mutex tenu à travers un `await`, canal jamais fermé |
| Mémoire qui explose | Canal non borné, frames accumulées |
| CPU saturé | Trop de workers, `rayon` non borné, threads OpenMP |
| TPU instable | Accès concurrent aux tenseurs, verrou manquant |
| Deux étapes en parallèle | Bug de réservation serveur |
| Sous-processus orphelin | `kill_on_drop` absent, SIGTERM non propagé |
| Téléchargements en rafale | `Semaphore` absent ou mal configuré |

## Règles

- Ne pas réintroduire `multiprocessing` Python ou OpenCV 5.0.
- Ne pas utiliser un canal non borné pour un flux vidéo.
- Ne pas tenir un verrou pendant une I/O longue.
- Ne pas lancer deux étapes matérielles, même sur des devices différents.
- Ne pas masquer un deadlock avec un timeout très long.
- Documenter tout nouveau patron de concurrence dans
  `memory-bank/systemPatterns.md`.
