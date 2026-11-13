# Banque centrale d'overlays de mission (A7)

Cette banque est le **repli** des assets de mission A7. Elle est consultée
uniquement quand le logo demandé par l'email n'existe pas dans le dossier
`docs/` du projet. Les fichiers déjà fournis par le client dans `docs/` restent
toujours prioritaires.

## Contenu

| Profil | Nombre | Versionné |
| --- | --- | --- |
| PNG 1080×1920 RGBA | 109 | oui |
| MOV ProRes 4444 `yuva444p12le` 1080×1920 | 8 | **non** (gitignorés, 2,2 Go) |
| `manifest.toml` | 1 | oui |
| `README.md` | 1 | oui |

Le manifeste fige le nom, le type, les dimensions, la durée MOV, le codec, le
pixel format, la présence audio, la taille et le SHA-256 de chaque asset.
Les binaires MOV restent locaux : un checkout neuf doit les recopier depuis la
source client avant d'utiliser le repli MOV.

## Déploiement

Depuis la racine du dépôt :

```bash
mkdir -p assets/mission_overlays
cp -a "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/mbox-to-json-2.0.0/logos_overlay/." \
  assets/mission_overlays/
```

La configuration de production active le repli avec un chemin relatif au
répertoire de lancement (le serveur travaille dans la racine du dépôt) :

```toml
[paths]
mission_overlays = "assets/mission_overlays"
```

`workflow-cli config check` échoue si ce chemin est configuré mais que le
répertoire est introuvable. Un chemin vide ou absent désactive le repli.

## Ordre de résolution A7

1. `docs/` local, fichier exact ;
2. `docs/` local, nom normalisé (casse, accents, ponctuation) ;
3. banque centrale, fichier exact ;
4. banque centrale, nom normalisé ;
5. extension implicite `.png` puis `.mov` (local puis banque) ;
6. table d'alias historique ;
7. `known_absent` (`lphdf.png`, `edc.png`) ;
8. `review` sans insertion.

Chaque asset retenu est revalidé par `probe_asset` : 1080×1920, alpha PNG ou
MOV ProRes compatible. La provenance est écrite dans le sidecar
`<stem>_mission.json` : `source = "local_docs"` ou
`source = "central_fallback"`.

Depuis D-069, un fichier local présent mais illisible (corrompu, tronqué) ou
hors profil déclenche une re-sonde de la banque avec la même sémantique de
résolution. Si l'asset central est sain, il est inséré en
`central_fallback` et le sidecar porte le warning
`local_asset_corrupted_fallback_used` ; si la banque n'a pas d'homologue
exploitable, la mission reste en `review`. Le brut, les assets de `docs/` et
`MediaSolution.jsx` ne sont jamais modifiés ; seul le sidecar A7 est ajouté.

## Chemins dans les `.ecproj`

La banque est déployée dans le dépôt pour que les projets sous `/mnt/cache`
puissent référencer un chemin relatif stable, par exemple :

```text
../../assets/mission_overlays/central.png
```

Le chemin est calculé depuis le dossier du projet assisté, jamais depuis le
répertoire courant.

## Vérification

```bash
# Dimensions et profil des MOV
ffprobe -v error -show_entries stream=codec_name,width,height,pix_fmt \
  -of default=nw=1 assets/mission_overlays/m6+.mov

# Empreinte d'un asset
sha256sum assets/mission_overlays/m6+.png
```

Comparer les valeurs avec `manifest.toml`. Toute mise à jour de la banque doit
régénérer le manifeste (mêmes champs, ordre alphabétique) et mettre à jour
`docs/lots/LOT-12.2-BANQUE-CENTRALE.md`.

## Limites

- Les MOV ne sont pas versionnés : sans copie locale, un logo MOV demandé en
  repli part en `review` (`missing_asset`), sans bloquer le pipeline.
- La sonde PNG lit l'en-tête, les dimensions et l'alpha ; un PNG dont l'en-tête
  est valide mais dont les données IDAT sont tronquées peut passer la sonde.
- Le projet brut et `docs/` ne sont pas réparés : le repli D-069 ne concerne
  que `_assist.ecproj` et son sidecar.
- Une banque absente ou illisible est ignorée à l'exécution ; le repli local
  puis la revue restent actifs. Seul `config check` signale une configuration
  invalide.
- Les alias restent une table fermée dans `workflow-core::mission` ; la
  présence d'un fichier dans la banque ne crée pas d'alias implicite.
