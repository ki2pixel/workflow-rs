# Lot 6 — STEP1 Rust : extraction et normalisation

| Champ | Valeur |
| --- | --- |
| Type | Lot |
| Statut | Référence |
| Date | 2026-10-24 |
| Portée | Extraction ZIP/RAR/TAR, assainissement des chemins et normalisation vidéo. |
| Voir aussi | `docs/contracts/CONTRAT-ENVIRONNEMENT.md`, `parity/DRIFT.md` |

## Objectif

Porter STEP1 sans modifier le serveur Python legacy :

- scanner `steps.step1.source_dir` sans récursion ;
- extraire les archives ZIP, RAR et TAR de façon sécurisée ;
- assainir chaque chemin de membre et refuser les traversées ;
- déplacer le contenu dans `<projet>/docs` ;
- normaliser les vidéos en MP4 / H.264 / progressif / 25 fps ;
- préserver les `.mov` alpha ou listés ;
- conserver la liste mensuelle des archives traitées et les journaux ;
- exposer STEP1 dans le serveur 5004 et dans les séquences.

Le lot supprime la dernière étape non portée. `POST /run/STEP1` ne renvoie plus
400 et aucune commande Python n'est appelée en repli.

## Architecture

```text
workflow-cli step1 run --source-dir <source>
  ├── archive::reset_processed_archives_if_needed
  ├── archive::find_archives_to_process
  ├── archive::extract_archive
  │     ├── sanitize::FilenameSanitizer
  │     ├── ZIP   (crate zip)
  │     ├── TAR   (crate tar + gzip/bzip2/xz)
  │     └── RAR   (unrar, comme rarfile)
  ├── normalize::discover_videos
  ├── normalize::analyze_video      (ffprobe)
  ├── normalize::normalize_video    (ffmpeg, GPU puis CPU)
  └── logs/step1/extract_archives_*.log
```

Le serveur construit la commande à partir de `steps.step1.source_dir` et
applique `steps.step1.timeout_s` (14 400 s par défaut, au lieu de 1 800 s par
défaut pour les autres étapes ; STEP7 et STEP8 disposent aussi de leur propre
timeout à 14 400 s).

## Configuration

```toml
[steps.step1]
source_dir = "/home/kidpixel/Téléchargements"
normalize = true
fps = 25
use_gpu = true
crf = 28
gpu_workers = 3
delete_archive = true
timeout_s = 14400
max_extracted_bytes = 107374182400
max_compression_ratio = 200
skip_preserved_mov = true
preserved_mov_names = ["m6+.mov"]
```

Les valeurs de normalisation (`fps`, `crf`, `gpu_workers`) sont validées au
chargement : `fps > 0`, `crf <= 51`, `gpu_workers > 0`, `timeout_s > 0`. Les
quotas `max_extracted_bytes` et `max_compression_ratio` doivent aussi être
strictement positifs. `source_dir` doit être non vide pour le serveur ; la CLI
accepte `--source-dir` en surcharge.

## Extraction

### Scan et liste mensuelle

- lecture de `logs/step1/processed_archives.txt` ;
- réinitialisation au changement de mois, avec sauvegarde
  `processed_archives_<mois>_backup_<horodatage>.txt` et marqueur
  `processed_archives.last_reset` ;
- scan non récursif des suffixes `.zip`, `.zipx`, `.rar`, `.tgz`, `.7z`,
  `.tar` contenant `steps.filter.keyword` (défaut `Camille`) ;
- comparaison des chemins canoniques avec la liste des archives traitées ;
- `.7z` reste trouvé puis refusé comme format non supporté, exactement comme
  Python.

Le scan reproduit le test `Path.suffix` de Python : `.tar.gz`, `.tar.bz2` et
`.tar.xz` ne sont pas sélectionnés par le scan (D-016). L'extraction TAR
compressée reste implémentée et testée au niveau module.

### Sécurité

`FilenameSanitizer` reprend `utils/filename_security.py` :

- normalisation Unicode NFKC ;
- suppression des composants vides, `.` et `..` ;
- suppression des caractères de contrôle et du NUL ;
- remplacement de `<>:"|?*` par `_` ;
- retrait des points et espaces en début et fin ;
- préfixe `safe_` pour les noms réservés Windows ;
- limite de 255 caractères par composant, extension conservée ;
- limite de 4 096 caractères pour le chemin complet ;
- repli `sanitized_file` pour un composant vide.

Après assainissement, `validate_extraction_path` refuse tout chemin absolu ou
comportant un composant parent. Les entrées non régulières d'un TAR
(symlinks, hardlinks, périphériques) sont ignorées et comptées comme problèmes
de sécurité, comme Python. Les liens ZIP sont écrits comme des fichiers
ordinaires, faute de support de lien dans `zipfile`.

Chaque archive est bornée par un budget de décompression :
`max_extracted_bytes` (100 Gio par défaut) pour le total extrait et
`max_compression_ratio` (200 par défaut) pour le rapport entre taille déclarée
et taille compressée. Un dépassement interrompt l'extraction ; le dossier
temporaire `_temp_*` est supprimé, l'archive reste en place et l'erreur est
remontée.

### Déplacement

- extraction dans `_temp_<projet>_<timestamp>` sous `projets_extraits/` ;
- suppression de `__MACOSX` ;
- aplatissement d'un unique dossier racine ;
- déplacement du contenu vers `<projet unique>/docs` ;
- écrasement explicite d'une cible existante, avec journal ;
- suppression garantie du dossier temporaire, y compris sur erreur ;
- suppression de l'archive source après succès (`delete_archive`).

Les collisions de nom de projet reçoivent un suffixe horodaté, puis un compteur
`-2`, `-3`, etc., comme Python.

## Normalisation

- découverte récursive de `.mp4`, `.mov`, `.avi`, `.mkv`, `.webm`, `.flv`,
  `.wmv` ;
- exclusion des artefacts `_temp_conversion`, `_converted`,
  `.temp_compress`, `.temp_normalize` ;
- séparation des `.mov` préservés (alpha réel, nom listé, ou sonde
  indéterminée) ;
- sonde `ffprobe` JSON pour le fps réel, les codecs vidéo/audio et les compteurs
  `nb_frames` / `duration` ;
- sonde `interlaced_frame` sur une seconde pour détecter le 1080i ;
- conformité = MP4 + H.264 + fps à ±0,1 + non entrelacé ;
- encodage GPU `h264_nvenc` (`p5`, `hq`, `cq`) puis repli CPU `libx264`
  (`medium`, `crf`) ;
- `yadif` pour l'entrelacement, `fps=25` si la cadence diverge ;
- copie audio pour `aac`, `mp3`, `ac3`, sinon AAC 192 kbit/s ;
- écriture dans `.temp_normalize.mp4`, vérification de collision, puis
  remplacement du fichier source ;
- progression `TOTAL_VIDEOS_TO_PROCESS` et `INTERNAL_PROGRESS`.

Une vidéo déjà conforme est ignorée, sauf avec `step1 normalize-only --force`.

## Journalisation

Chaque exécution crée `logs/step1/extract_archives_YYYYmmdd_HHMMSS.log`. Les
lignes sont aussi écrites sur la sortie standard pour le serveur, qui les parse
et les expose via `/status/STEP1`, `/get_specific_log/STEP1/0` et
`processed_archives.txt`.

## Validation

- `cargo test --workspace` : 121 tests, dont les tests STEP1 (assainissement,
  noms de projets, collisions, réinitialisation mensuelle, ZIP avec traversée,
  TAR gzip, scan, extraction marquée puis supprimée, second passage ignoré) ;
- archive synthétique réelle `13_Camille_test.zip` : extraite, `ready.mp4`
  ignorée, `legacy.avi` et `nonalpha.mov` converties en H.264 25 fps,
  `m6+.mov` préservé, archive supprimée, journal et liste écrits ;
- archive client réelle `153 Camille.zip` (1,9 Go, entrée racine `/`, six
  fichiers aux noms Unicode) : extraction seule en 2,4 s, six fichiers de
  tailles identiques, md5 du PNG identique, zéro problème de sécurité ;
- second passage sur la même archive : aucun retraitement, grâce à
  `processed_archives.txt` ;
- réinitialisation mensuelle réelle : marqueur passé de `2026-09` à `2026-10`,
  sauvegarde `processed_archives_2026-09_backup_*.txt`, liste vidée puis
  archive réextraite ;
- parité Python sur une même archive `20_Camille_parity.zip`, avec CPU et un
  worker : les quatre artefacts (`ready.mp4`, `legacy.mp4`, `nonalpha.mp4`,
  `m6+.mov`) sont identiques au bit près (md5). La sortie ne diffère que par le
  doublon `TOTAL_VIDEOS_TO_PROCESS` de Python et le JSON final du CLI ;
- `scripts/validate-step1.py` reproduit cette comparaison dans un arbre
  temporaire, sans écrire dans `workflow_mediapipe` : `4/4 fichiers identiques` ;
- serveur 5004 : `POST /run/STEP1` passe à `completed` (code 0), log STEP1
  consultable, projet `15 Camille/docs/legacy.mp4` produit ;
- `cargo fmt --check` et `cargo clippy --workspace --all-targets -D warnings`
  passent.

## Limites

- Les archives RAR dépendent de la commande `unrar` (comme `rarfile`) ; aucun
  mot de passe n'est accepté, et un nom de membre contenant un saut de ligne
  n'est pas représentable dans la sortie `unrar lb`. Le chemin RAR a été
  exercé manuellement sur une archive RAR5 de test (deux fichiers extraits à
  l'identique). Les anciens RAR dont `unrar` sort en code 3 restent traités
  comme des échecs, comme `rarfile`.
- Les noms TAR non UTF-8 sont décodés en lossy (D-017).
- Le scan ne sélectionne pas les suffixes composés `.tar.gz`, `.tar.bz2`,
  `.tar.xz` (D-016), par fidélité au comportement Python.
- `unrar`, `ffmpeg` et `ffprobe` restent des dépendances système ; le lot ne
  les embarque pas.
- La validation Windows/After Effects reste hors périmètre.
