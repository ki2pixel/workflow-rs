#!/usr/bin/env bash
# Synchronise l'état Python legacy et l'état Rust.
#
# À lancer après arrêt des deux serveurs, avant de démarrer le backend cible.
# Sans --apply, la commande affiche seulement le plan (dry-run).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG="${WORKFLOW_RUST_CONFIG:-$ROOT/config/config.toml}"
CLI="${WORKFLOW_RUST_CLI:-$ROOT/target/release/workflow-cli}"

usage() {
    cat <<EOF
Usage: $0 [options]

Fusionne download_history, logs/step1/processed_archives.txt, les archives
non traitées et les analyses STEP7 entre Python et Rust. Les deux serveurs
doivent être arrêtés.

Options principales :
  --apply                 écrit les changements (sinon simulation)
  --direction both|python-to-rust|rust-to-python
  --history merge|skip
  --processed merge|skip
  --downloads copy|move|skip
  --step7 copy|link|skip
  --normalize             réécrit les URL avec la normalisation Rust
  --allow-running         diagnostic en lecture seule, serveurs actifs acceptés
  --quiet                 résumé lisible au lieu du JSON
  --backup-dir <path>     répertoire de sauvegarde

Exemples :
  $0                                  # plan
  $0 --apply                          # fusion réelle
  $0 --apply --step7 link             # analyses STEP7 en hardlinks
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    usage
    exit 0
fi

if [[ ! -x "$CLI" ]]; then
    echo "workflow-cli absent ou non exécutable: $CLI" >&2
    echo "Construire d'abord: cargo build --release -p workflow-cli" >&2
    exit 1
fi

exec "$CLI" --config "$CONFIG" sync "$@"
