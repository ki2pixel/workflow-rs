# Lot 7 — Intégration réelle et bascule contrôlée

## Objectif

Vérifier que les étapes Rust s'enchaînent sur des données de production sans
écrire dans les sorties Python, puis que le serveur 5004 démarre et s'arrête
pendant que le legacy reste disponible sur 5003 pour le rollback.

## Séquence STEP2 → STEP8 sur 244

La copie de travail est `/tmp/opencode/full` : `244 Cvmille` est copié depuis
`/mnt/cache/old`, les CSV/JSON de `docs/` sont supprimés pour forcer le
recalcul, et la configuration pointe `base`, `projects`, `logs`, `finalized` et
`cache_root` sous `/tmp`. Le serveur de test écoute sur 5006 ; la production
`/mnt/cache/old/244 Cvmille` n'est jamais écrite.

Séquence `STEP2 → STEP8` lancée par `POST /run_custom_sequence` :

| Étape | Résultat |
| --- | --- |
| STEP2 | 2/2 vidéos, 78 scènes par vidéo, 4 920 trames |
| STEP3 | 2/2, YAMNet Split Edge TPU, ECAPA-TDNN CPU, 30 et 29 segments |
| STEP4 | 2/2, 4 920 trames par vidéo |
| STEP5/STEP6 | 0 projet : le dossier s'appelle `244 Cvmille`, le mot-clé est `Camille` |
| STEP7 | 1 projet finalisé vers `/tmp/opencode/full/out` |
| STEP8 | 2/2 mp4 traités, `.ecproj` produits |

Le saut STEP5/6 est attendu : le filtre Rust reprend le mot-clé Python
`Camille`, et `Cvmille` ne le contient pas. Les mtimes vérifiés de la copie de
production sont restés au 2026-10-21.

## STEP5/6 sur les artefacts Rust

Pour combler ce saut, les artefacts STEP2–4 archivés ont été replacés dans un
projet `244 Camille/docs`, puis :

```sh
workflow-cli --config ... step5 project --work-dir ... --keyword Camille
workflow-cli --config ... step6 project --work-dir ... --keyword Camille
```

STEP5 réduit `_audio.json` et `_tracking.json` en place ; STEP6 écrit
`_ae.json` pour les deux vidéos. Pour la vidéo d'origine, les trois JSON sont
identiques sémantiquement à la production :

| Fichier | MD5 canonique (production et Rust) |
| --- | --- |
| `_ae.json` | `a61bcc1824f6f4ecc3a0da1ce47f79cf` |
| `_audio.json` | `7d98ad1bd5431f4f4c4ec2996f97cf0c` |
| `_tracking.json` | `c04b866fe15736213df9ae77e8f399f4` |

Le MD5 canonique est calculé après `json.dumps(..., sort_keys=True,
separators=(',', ':'))`. Les octets bruts diffèrent uniquement par l'ordre des
clés et l'indentation : Rust écrit `fps` et `total_frames` avant
`frames_analysis`, conformément à la correction assumée de STEP5.

## Bascule contrôlée 5003 / 5004

Python legacy a été lancé depuis une copie de code sous
`/tmp/opencode/switch-python`, avec la base, les projets, les archives, les
logs, le cache et les téléchargements sous `/tmp`, `DRY_RUN_DOWNLOADS=true` et
`ENABLE_GPU_MONITORING=false`. Aucun téléchargement et aucune écriture dans
les données de production.

```sh
./scripts/switch-rust.sh check
./scripts/switch-rust.sh start
./scripts/switch-rust.sh status
./scripts/switch-rust.sh stop
```

Résultats :

- `check` voit Python sur 5003 et le signale comme rollback disponible ;
- `start` lance Rust sur 5004 et laisse Python sur 5003 ;
- index Rust `200`, `GET /api/ping` répond `runtime: rust`, les huit étapes
  sont `idle`, `/sound-design/<fichier>` répond `200` ;
- `/api/audio/lemonfox` répond `404` sur Rust : cette route reste dans le
  legacy Python, comme prévu ;
- `stop` ferme 5004 ; Python répond toujours `200` et `pong` sur 5003.

La bascule du trafic public (reverse proxy, DNS ou port) reste une opération
externe : le script ne touche jamais au port 5003.

## Limites

- STEP1 n'a pas été inclus dans la séquence réelle STEP2 → STEP8. Il est validé
  séparément par `scripts/validate-step1.py` (4/4 fichiers identiques) et par
  un `POST /run/STEP1` réel sur le serveur 5004.
- Le RAR a été exercé manuellement sur une archive RAR5 de test ; les anciens
  RAR dont `unrar` sort en code 3 restent des échecs, comme `rarfile`.
- La compilation Windows du pont, le basculement du JSX After Effects et la
  validation STEP8 sur la machine Windows sont abandonnés (D-022) : la cible
  est Linux et le panneau CEP continue d'appeler Python.
- La bascule publique est décrite dans `SWITCH-PUBLIC.md` ; le routage reste
  une décision d'exploitation.
