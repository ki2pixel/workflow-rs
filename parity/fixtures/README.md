# Fixtures de parité

Ce dossier contient les sorties de référence. Les fichiers volumineux ne sont
pas versionnés.

`checksums.json` est la source de vérité : il associe chaque cas à ses fichiers
attendus, leur SHA-256, leur taille et le chemin de production d'origine.

Pour reconstruire une référence :

```bash
coral_env/bin/python tools/parity/parity.py snapshot --case real_244_step6
```

Pour un cas synthétique, lancer d'abord la mesure avec `--snapshot` :

```bash
coral_env/bin/python tools/parity/parity.py run --case cuts_step2 --snapshot
```
