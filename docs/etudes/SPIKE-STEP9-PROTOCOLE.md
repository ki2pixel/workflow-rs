# Protocole de spike — STEP9 post-STEP8

> **Statut** : protocole d'expérimentation. A5/A6 sont industrialisés depuis le
> 2026-10-27 ; les scénarios B/C restent des spikes.
> **Étude associée** : `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`.
> **Décisions** : D-029 à D-032 (`memory-bank/decisionLog.md`).
> **Date** : 2026-10-26, mise à jour le 2026-10-27.

## 1. Objectif

Valider ou invalider, sur des copies isolées, les trois hypothèses suivantes :

| ID | Hypothèse | Critère de succès |
| --- | --- | --- |
| **H1** | Un post-traitement déterministe (A) reproduit l'essentiel des gestes de cadrage/nettoyage : letterbox, overlays, keyframes, transitions | ≥ 70 % de ce delta, hash de projet stable, zéro faux positif sur les contrôles propres |
| **H2** | Un agent QA read-only (B) détecte les non-conformités avec une précision suffisante | précision ≥ 95 %, rappel ≥ 90 % sur logos/sous-titres |
| **H3** | Un agent co-monteur (C) apporte un gain sémantique mesurable sans régression | préférence aveugle ≥ 60 % vs A+B, pas de régression qualité |

**Périmètre A** : A0 profilage, A1 letterbox, A2 overlays/sous-titres source,
A3 keyframes, A4 transitions, A5 rapport. Le relink est un contrôle optionnel.
Le placement des logos/jingle, l'extension de composition et le rendu H.264
restent **hors périmètre** : directives client et export GPU humain.

Le spike ne cherche pas à prouver que C est « prêt pour la production ». Il
cherche à savoir si C mérite un investissement supplémentaire.

## 2. Périmètre et isolation

### 2.1 Ce qui est autorisé

- lire `/mnt/cache/298 Camille.zip`, `300 Camille.zip` et les `_GOOD.zip` ;
- copier les archives dans `/tmp/opencode/step9-spike/` ;
- exécuter `workflow-cli step8`, `effectcraft-cli` et `ffmpeg` sur les copies ;
- écrire des rapports, labels, frames et vidéos sous `/tmp/opencode/step9-spike/` ;
- utiliser une configuration TOML copiée avec le webhook désactivé.

### 2.2 Ce qui est interdit

- écrire dans `/mnt/cache`, `runtime/`, `assets/`, `crates/` ou
  `/home/kidpixel/workflow_mediapipe` ;
- lancer le serveur `:5004` ou le moniteur webhook ;
- exécuter STEP1/2/3/4 matériels sur la production ;
- modifier les contrats `_ae.json`, `_audio.json`, CSV ou le panel ;
- envoyer des frames à un service cloud sans accord explicite ;
- écraser un projet `_GOOD` : il est **lecture seule**.

### 2.3 Arborescence

```text
/tmp/opencode/step9-spike/
├── 00-inputs/
│   ├── 298 Camille.zip
│   └── 300 Camille.zip
├── 00-refs/
│   ├── 298 Camille_GOOD.zip
│   └── 300 Camille_GOOD.zip
├── 01-baseline/
│   ├── 298/
│   └── 300/
├── 02-labels/
│   ├── labels-298.json
│   ├── labels-300-joseph.json
│   └── labels-300-katia.json
├── 03-scenario-a/
│   ├── a5_report.py
│   └── a5_eval.py
├── 04-scenario-b/
├── 05-scenario-c/
├── 06-corpus/
│   ├── inventory.json
│   ├── labels/
│   ├── profiles/
│   ├── a5/
│   └── RAPPORT-CORPUS.md
├── logs/
└── config/
    └── config.spike.toml
```

### 2.4 Vérification d'intégrité

```sh
set -euo pipefail
ROOT=/tmp/opencode/step9-spike
mkdir -p "$ROOT"/{00-inputs,00-refs,01-baseline,02-labels,03-scenario-a,04-scenario-b,05-scenario-c,logs,config}

cp -n "/mnt/cache/298 Camille.zip"        "$ROOT/00-inputs/"
cp -n "/mnt/cache/300 Camille.zip"        "$ROOT/00-inputs/"
cp -n "/mnt/cache/298 Camille_GOOD.zip"   "$ROOT/00-refs/"
cp -n "/mnt/cache/300 Camille_GOOD.zip"   "$ROOT/00-refs/"

sha256sum "$ROOT"/00-inputs/*.zip "$ROOT"/00-refs/*.zip | tee "$ROOT/logs/sha256-inputs.txt"
```

Contrôle : les sommes doivent être identiques avant et après le spike. Aucun
`unzip` ne doit écrire hors de `$ROOT`.

## 3. Configuration isolée

Copier `config/config.toml` et vérifier :

```toml
[webhook]
enabled = false
dry_run = true

[paths]
base = "/tmp/opencode/step9-spike/runtime"
projects = "/tmp/opencode/step9-spike/runtime/projets_extraits"
archives = "/tmp/opencode/step9-spike/runtime/archives"
logs = "/tmp/opencode/step9-spike/logs"
downloads = "/tmp/opencode/step9-spike/runtime/downloads"
history_db = "/tmp/opencode/step9-spike/runtime/download_history.sqlite3"
cache_root = "/tmp/opencode/step9-spike/cache"
finalized = "/tmp/opencode/step9-spike/out"
```

Le spike ne lance que STEP8 : le verrou matériel `/tmp/workflow-rs-hw.lock`
n'est pas nécessaire. Si un test dérive vers STEP1–4, arrêter et isoler.

## 4. Phase 0 — Baseline et labels

### 4.1 Baseline STEP8

```sh
ROOT=/tmp/opencode/step9-spike
CASE="$ROOT/01-baseline/298"
mkdir -p "$CASE"
unzip -q "$ROOT/00-inputs/298 Camille.zip" -d "$CASE"

cd /home/kidpixel/workflow-rs
./target/release/workflow-cli step8 run \
  --config "$ROOT/config/config.spike.toml" \
  --folder "$CASE/298 Camille" \
  --logs-dir "$ROOT/logs" \
  --timeout-s 3600
```

Le zip brut contient déjà un `.ecproj` de production. Avant de relancer
STEP8, le copier dans `01-baseline/298/archive/` : c'est la baseline
historique. Relancer STEP8 dans un sous-dossier `regen/` (ou après avoir
déplacé `projets/`) pour vérifier la reproductibilité sans écraser la
baseline.

Répéter pour 300 (deux vidéos). Noter :

- durée de STEP8 ;
- nombre de segments ;
- `MS_BATCH_RESULT` ;
- chemin du `.ecproj` ;
- toute ligne `WARN`.

### 4.2 Baseline rendu (référence pour B/QC, pas une sortie A)

```sh
effectcraft-cli --project "$CASE/298 Camille/projets/The 100 extrait 2_9x16.ecproj" \
  render --out "$CASE/baseline.mp4" --format h264 --rate-control bitrate --bitrate 10000 --progress

ffprobe -v error \
  -show_entries stream=width,height,r_frame_rate \
  -show_entries format=duration,bit_rate,size \
  -of default=nw=1 "$CASE/baseline.mp4"
```

### 4.3 Labels de référence

Les `_GOOD.zip` contiennent les projets finaux. Les extraire en lecture seule
et produire un JSON de labels :

```sh
mkdir -p "$ROOT/02-labels/good-298"
unzip -q "$ROOT/00-refs/298 Camille_GOOD.zip" -d "$ROOT/02-labels/good-298"
```

Le script de labels doit extraire, pour chaque calque `Segment` :

- `id`, `name`, `in_point`, `out_point`, `stretch`, `parent` ;
- `anchor`, `position` (valeur + clés), `scale` ;
- durée de composition, fps, résolution ;
- calques non-`Segment` (logos, jingle, null) ;
- assets référencés.

Squelette Python (lecture seule, à placer dans `03-scenario-a/` ou
`02-labels/`) :

```python
import json, re, sys
from pathlib import Path

TICKS = 254_016_000_000  # timebase After Effects

def prop(layer, match):
    def walk(node):
        if node.get("node") == "Prop" and node.get("match") == match:
            return node
        for child in node.get("children", []) or []:
            found = walk(child)
            if found:
                return found
        return None
    return walk(layer.get("props", {}))

def main(path):
    data = json.loads(Path(path).read_text())
    comps = [i for i in data["items"].values()
             if i.get("kind", {}).get("type") == "Comp"]
    assert len(comps) == 1
    comp = comps[0]
    out = {
        "project": path,
        "comp": {
            "name": comp["name"],
            "width": comp["kind"]["width"],
            "height": comp["kind"]["height"],
            "fps": comp["kind"]["frame_rate"],
            "duration_s": comp["kind"]["duration"] / TICKS,
        },
        "layers": [],
    }
    for layer in comp["kind"]["layers"]:
        anchor = prop(layer, "anchor")
        position = prop(layer, "position")
        scale = prop(layer, "scale")
        out["layers"].append({
            "id": layer["id"],
            "name": layer["name"],
            "source": layer.get("source", {}).get("item"),
            "in_s": layer["in_point"] / TICKS,
            "out_s": layer["out_point"] / TICKS,
            "parent": layer.get("parent"),
            "anchor": anchor.get("value", {}).get("v") if anchor else None,
            "position": position.get("value", {}).get("v") if position else None,
            "position_keys": [
                {"t": k["time"] / TICKS, "v": k.get("value", {}).get("v")}
                for k in (position.get("keys", []) if position else [])
            ],
            "scale": scale.get("value", {}).get("v") if scale else None,
        })
    print(json.dumps(out, indent=2, ensure_ascii=False))

if __name__ == "__main__":
    main(sys.argv[1])
```

Produire un label par projet, puis un fichier agrégé :

- `labels-298.json` ;
- `labels-300-joseph.json` ;
- `labels-300-katia.json`.

### 4.4 Métriques de baseline

| Métrique | Comment | Cible |
| --- | --- | --- |
| Segments | compter les calques `Segment` | 24 / 36 / 60 |
| Keyframes | compter les clés `position` | 16 / 25 / 31 |
| Ancre | comparer brut vs GOOD | delta 0 px |
| Scale | comparer brut vs GOOD | 180 vs 207 |
| Overlays | compter les calques non-`Segment` | 2 logos + 1 jingle (298) |
| Durée | comp vs somme des segments | +3 s (298) |
| Rendu | ffprobe | 1080×1920, 25 fps, 10 ± 0,5 Mbit/s |

### 4.5 Résultats Phase 0 (2026-10-26)

La Phase 0 du protocole a été exécutée sur copies isolées. Résultats mesurés :

| Mesure | 298 | 300 JOSEPH | 300 KATIA |
| --- | --- | --- | --- |
| STEP8 reproductible | oui (25 calques) | oui (37) | oui (59) |
| Ancre modifiée | 0 | 0 | 0 |
| Bande basse détectée | aucune | 108 px | 108 px |
| Scale A2 (marge 5 %) | 180,0 % (no-op) | **207,4 %** | **207,4 %** |
| Scale humain | 180 % | 207 % (35), 254,7 % (1) | 207 % (59), 287,6 % (1) |
| Cadrage statique ≤ 50 px du brut | 13/23 | 7/36 | 24/60 |
| Clés ≤ 60 px d'un objet suivi | 9/16 | 11/25 | 8/31 |

**Décisions issues de la Phase 0 :**

- **A2 est validé** : 207,4 % vs 207 %, no-op correct sur 298. La marge de
  5 % est mesurée, pas arbitraire.
- **A1 est un no-op** sur ce corpus (aucun letterbox) ; à valider sur un vrai
  2.39:1 ou 4:3.
- **A2 est déterministe** : deux applications sur une copie de 300 JOSEPH
  donnent le même SHA-256, sans toucher à l'ancre ni à la position.
- **A3 doit être conservateur** : 39 % des clés seulement sont traçables à
  ≤ 60 px. La règle « amplitude > 100 px + suivi continu » a une précision
  de 0,62 sur 298, 0,29 sur JOSEPH, 0,20 sur KATIA. Le prototype 298 obtient
  précision 0,50, rappel 0,60 et RMSE médian 239 px : il rate les suivis
  troués et ajoute des clés sur des segments statiques. En dessous du seuil,
  le segment reste statique et part en `review`.
- **A4 est validé sur le mécanique** : fusion < 0,5 s (1/1) et cut dur
  manqué (1/1 sur 298) ; les 2 splits de KATIA ne sont pas des cuts visuels
  (corrélation 0,99) et restent en `review`.
- **Le re-cadrage statique est un angle mort** : sur 300, 29/36 et 35/60
  segments sont à plus de 100 px du brut centré, sans keyframe. A ne peut
  pas les retrouver avec `_ae.json` seul ; B (ou l'humain) doit les voir.

Rapport détaillé : `/tmp/opencode/step9-spike/01-baseline/RAPPORT-PHASE0.md`.

### 4.6 Corpus élargi (2026-10-27)

Le corpus a été porté à **19 paires / 32 vidéos exploitables** : 179, 181,
187, 191, 208, 211, 221, 223, 227, 266, 268, 270, 276, 286, 287, 295, 298,
300, 303. Il couvre les strates du §8.3 : overlays/mouvement (8 paires),
letterbox (268), cas complexe (191), mixte (227), « sans piège » (5 paires),
template SDM (295) et les références 298/300.

Deux paires restent incomplètes et ne produisent pas de labels fins :

- **179** : GOOD sans `.ecproj` (`.aep`/`.prproj` seulement), 3 vidéos ;
- **287** : 3 bruts, 1 seul `.ecproj` GOOD (LFAUIT) ; les 2 Totally Spies
  n'ont pas de référence.

Le GOOD de **295** a été fourni le 2026-10-27. Il partage le même `_ae.json`
et le même `_tracking.json` que le brut (SHA-256 identiques) : c'est un
contrôle propre pour le template SDM, mesuré au §5.7.

La 2CV de 211 est exclue (piste audio/vidéo suspecte, 54 min). Le nom de
fichier de 266 avec espace final a été normalisé dans le pipeline de labels.

**Dérive de baseline — cause identifiée (2026-10-27)** : le delta vient de
**STEP4/5 (tracking)**, pas de STEP6/8. Les `_GOOD` 187/181 datent du
2026-10-15/16 et utilisent le tracking legacy ; les bruts du 2026-10-27
utilisent le tracking Rust. Preuve : le STEP6 Rust alimenté par le
`_tracking.json` legacy reproduit le `_GOOD/_ae.json` à l'identique
(4355/4355 frames sur 187, 2979/2979 sur 181). Le tracking Rust détecte
moins de personnes (164 vs 511 sur 187) et change les ids. La bascule est
datée : le commit legacy `0e9765b` du **2026-10-16 19:01** a introduit
`STEP4_TPU_FAST=1` et `object_min_missing=25`, après les GOOD. Le Rust
reproduit le mode *fast* actuel, pas le mode pré-fast des GOOD. Un run Rust
`--object-min-missing 1` remonte à 1722 frames personne contre 1623 legacy
(au lieu de 276), mais ne suffit pas à retrouver les ancres : le delta médian
vs GOOD reste 64,33 px, contre 8,39 px avec le `_tracking.json` legacy. Le
gate explique la couverture personne, pas le delta d'ancre ; le résidu vient
de la détection de visage et de la sélection d'objet.

**Portée** : cette reconstruction est un **outil de diagnostic historique**,
pas la méthode du corpus. La référence de production est **STEP1–8 Rust** :
les bruts, le tracking et les ancres sortent du même pipeline, puis
`MediaSolution.jsx` arbitre le recentrage. Les GOOD 181/187 sont des
références historiques (tracking legacy, avant le fast et avant
`segment_recommendations`) ; ils ne doivent pas être mélangés aux bruts Rust
dans une base de travail. Le GOOD 295 est le seul contrôle propre. La parité
STEP4/5 legacy/Rust est un chantier externe, hors-scope STEP9 : le tracking
Rust est la référence de production. La recette ci-dessous ne sert qu'à
expliquer le delta passé, pas à générer de nouveaux corpus.

Recette de diagnostic (ne pas utiliser pour le corpus) :

```sh
workflow-cli step6 build \
  --tracking "<archive>_tracking.json" \
  --audio "<archive>_audio.json" \
  --csv "<stem>.csv" \
  --output "<work>/docs/<stem>_ae.json"
workflow-cli step8 run --folder "<work>" --logs-dir /tmp/opencode/step9-spike/logs
```

#### Mesures

| Mesure | Valeur |
| --- | ---: |
| Segments bruts analysés | 1059 |
| Segments keyframés | 145 |
| Clés humaines | 520 |
| Clés à ≤ 60 px d'un objet suivi | 278 (53,5 %) |
| Segments statiques | 907 |
| Statiques à ≤ 50 px du brut | 458 (50,5 %) |
| Statiques à 50–100 px | 86 (9,5 %) |
| Statiques à > 100 px | 363 (40,0 %) |

| Catégorie | Vidéos | Segments | Keyframés | Clés ≤ 60 px | Statiques ≤ 50 px |
| --- | ---: | ---: | ---: | ---: | ---: |
| Overlays/mouvement | 15 | 459 | 69 | 130/248 (52 %) | 155/359 (43 %) |
| Letterbox (268) | 1 | 37 | 8 | 15/28 (54 %) | 15/32 (47 %) |
| Complexe (191) | 5 | 202 | 32 | 93/132 (70 %) | 104/184 (57 %) |
| Mixte (227) | 1 | 16 | 0 | — | 9/16 (56 %) |
| Sans piège | 7 | 227 | 13 | 12/40 (30 %) | 132/220 (60 %) |
| Référence (298/300) | 3 | 118 | 23 | 28/72 (39 %) | 43/96 (45 %) |

#### A1 — letterbox

- **191** : 152 px de barres noires en haut et 128 px en bas (hauteur active
  800 px) → `1920/800 = 240,0 %`. Le monteur a utilisé 255 % ; avec la marge
  de 4 % du prototype, 249,6 %, soit 2,1 % d'écart. Avec une marge de 6,25 %,
  on retrouve 255 %. Validé, mais la marge de 4 % sous-estime le geste humain
  de 191.
- **268** : deux hauteurs dominantes par segment, 1072 px → 179,1 % (humain
  180 %) et 864 px → 222,2 % (humain 222,6 à 239,7 %). Les écarts de 15–17 %
  viennent des segments partiellement letterboxés : le monteur prend le
  **maximum** de letterbox sur le segment, pas le mode.
- **Limite** : les 8 derniers segments de 268 n'ont aucune barre noire
  détectée mais sont scalés à 204–224 % (bords flous ou colorés, archive
  4:3). `cropdetect` ne les voit pas ; ils doivent aller en `review` ou
  utiliser un détecteur de bords à faible détail.

#### A2 — overlays

- **187 DISNEYWORLD** : bannière basse 924–1032 (108 px) → 207,4 %, humain
  207 %. Correct.
- **187 ECOLE** : le détecteur médiane + gradient donne 692–1032 (340 px)
  → 260 %, **faux positif** dû à une zone statique de décor au-dessus de la
  bannière.
- **191** : la zone détectée (788–960, 172 px) est un **logo de coin**
  « CANCER LA VIE DEVANT », pas une bannière pleine largeur. A2 doit le
  signaler en `review`, pas zoomer.
- **295** : le logo « SCÈNES DE MÉNAGE » est un PNG séparé (`sdm.png`), pas
  incrusté dans la source ; il appartient au template A6 et ne doit pas être
  traité comme un logo de chaîne à ajouter ni comme une bannière A2.

Le détecteur doit distinguer : bannière pleine largeur (> 50 % de la largeur)
→ scale global ; logo de coin (< 30 %) → `review` ; zone statique de décor →
ignorer. **A2 était fragile sur les cas non couverts** (bannière sombre,
animée, en haut de l'image) ; le détecteur A5 sépare correctement la bannière
claire de 187 DISNEYWORLD/ECOLE (206,6 % vs 207 %) et ne confond plus le décor
de 187 ECOLE, mais il n'est production-ready que sur ce type de bannière.

**Mise à jour v2 (2026-10-27)** : le détecteur scanne toute la hauteur,
détecte les bandes sombres par paires de bords, mesure la stabilité
géométrique des bords et classe haut/sombre/glissant/logo/partiel/décor en
`review`. Validation synthétique 11/11 ; 187 reste à 206,6 %, 181 Concierge
est en `noop`. Voir `06-corpus/a5v2/RAPPORT-A5-V2.md`.

#### A3 — keyframes invalidé en auto

Règle testée : amplitude du centroïde > 100 px, trous < 30 %, durée ≥ 1 s.

| Catégorie | TP | FP | FN | Précision | Rappel |
| --- | ---: | ---: | ---: | ---: | ---: |
| Overlays/mouvement | 12 | 58 | 61 | 0,17 | 0,16 |
| Letterbox | 2 | 5 | 6 | 0,29 | 0,25 |
| Complexe | 15 | 27 | 16 | 0,36 | 0,48 |
| Mixte | 0 | 1 | 0 | 0,00 | 0,00 |
| Sans piège | 1 | 35 | 11 | 0,03 | 0,08 |
| Référence | 4 | 12 | 18 | 0,25 | 0,18 |
| **Total** | **34** | **138** | **112** | **0,20** | **0,23** |

Sur les vidéos « sans piège », la règle ajoute des clés à **35 segments** où
le monteur n'en a mis qu'une : c'est le pire résultat possible pour un module
censé ne rien casser. A3 ne doit **pas** être auto-appliqué ; il devient un
générateur de candidats en `review`, ou une option explicite par projet.

#### A4 — transitions

Validé sur 298 (fusion < 0,5 s 1/1, cut dur 1/1 à 53,68 s vs 53,64 s). Sur le
corpus élargi, les splits/merges restent à mesurer segment par segment ; les
splits sémantiques de KATIA (corrélation 0,99) restent en `review`.

#### SDM 295 — template déterministe (A6)

Le GOOD 295 montre un template constant : un plan principal resserré
(84,35 %, 1080×911) au-dessus d'un plan 16:9 complet (56,29 %, 1080×608),
la même image floutée en fond sur toute la hauteur, plus le logo `sdm.png`.
Ce n'est ni A1, ni A2, ni A3 : c'est un **template de composition** (A6),
configurable et appliqué à toute la vidéo. Il est déterministe et récurrent,
et le prototype `06-corpus/a6_template.py` le reproduit au pixel près
(§5.7).

Rapport détaillé : `/tmp/opencode/step9-spike/06-corpus/RAPPORT-CORPUS.md` et
table par vidéo `06-corpus/table-corpus.md`.

## 5. Phase A — Post-traitement déterministe

### 5.1 Périmètre

Implémenter un prototype **hors pipeline**, par exemple
`03-scenario-a/postprocess.py` ou `03-scenario-a/postprocess.jsx`, qui :

1. lit le `.ecproj` baseline, les artefacts `_ae.json` / `_tracking.json`,
   le CSV et la source vidéo ;
2. produit `video_profile.json` (A0) avec les signaux et leur confiance ;
3. calcule les décisions A1–A4 uniquement si le signal est détecté ;
4. applique les modifications via `effectcraft-cli script` ou MCP, sur une
   copie ;
5. écrit `post.json`, `review.json` et `audit.json` ;
6. **ne rend pas le MP4**, ne place pas les logos/jingle et n'allonge pas la
   composition.

Ne pas modifier `MediaSolution.jsx` pour le spike : le prototype doit rester
isolé et comparable au baseline.

### 5.1.1 A0 — Profilage

Avant toute modification, produire un profil de la vidéo :

```sh
ffprobe -v error -show_entries stream=width,height,r_frame_rate,codec_name \
  -show_entries format=duration -of json source.mp4

ffmpeg -hide_banner -ss 0 -i source.mp4 -t 60 \
  -vf cropdetect=24:16:0 -f null - 2>&1 \
  | grep -o "crop=[0-9:]*" | sort | uniq -c | sort -rn | head
```

Compléter avec :

- médiane temporelle / gradient / OCR sur la bande basse et les coins ;
- `_ae.json` / `_tracking.json` : présence, `centroid_x`, confiance,
  `is_speaking`, `tier`, trous de suivi, changements d'identité ;
- CSV STEP2 + frame-diff / histogrammes autour des frontières ;
- audio : silence, changement de locuteur, musique.

Sortie `video_profile.json` :

```json
{
  "schema": 1,
  "source": "exemple.mp4",
  "letterbox": {"active_height": 1072, "confidence": 0.98},
  "source_overlays": [
    {"kind": "banner", "bbox": [0, 955, 1920, 70], "persistence": 0.97,
     "confidence": 0.94}
  ],
  "motion": [
    {"segment": "5", "path_px": 870, "gaps": 0, "confidence": 0.81}
  ],
  "transitions": [
    {"at_s": 53.64, "kind": "tracking_loss", "confidence": 0.76}
  ]
}
```

Si un signal est sous le seuil, A ne fait rien et écrit `review`. La plupart
des vidéos n'ayant aucun signal, le profil est vide et A est un no-op.

### 5.2 A1 — Letterbox

**Statut mesuré (2026-10-27)** : validé sur le letterbox noir global (191) et
par segment (268, max sur le segment) ; incomplet sur les bords non noirs
(268, 8 segments) → `review`.

```sh
ffmpeg -hide_banner -ss 0 -i source.mp4 -t 60 \
  -vf cropdetect=24:16:0 -f null - 2>&1 \
  | grep -o "crop=[0-9:]*" | sort | uniq -c | sort -rn | head
```

Calcul :

```text
scale = max(1080 / largeur_active, 1920 / hauteur_active) * 100
```

Vérifier par `render_frame` à 3 timecodes (début, milieu, fin).

**Couverture après scale** : le scale seul ne garantit pas que la zone active
remplit la comp. Si l'ancre du sujet n'est pas au centre de la zone active, le
layer se déplace et une barre peut rester. Mesuré sur 191 : 249,6 % supprime
la barre du bas mais laisse 19,5 % de barre en haut ; avec une compensation de
Position Y de −367,8 px enfant (−417,8 px en comp, le parent Null vaut −50 px),
les barres disparaissent. Dans ce cas, compenser la **Position Y** (jamais
l'Ancre) et signaler le cadrage vertical en `review` si le décalage dépasse
100 px. Le contrôle par frames est donc partie intégrante de A1, pas une
option.

**Implémentation v2 (2026-10-27)** :

- hauteur active = **10e percentile** des crops du segment (barres les plus
  hautes hors flash isolé), et non le crop dominant ;
- vérification de **couverture mathématique** de tous les crops après scale
  et compensation Y : `top <= 0`, `bottom >= 1920`, `left <= 0`,
  `right >= 1080`, tolérance 0,5 px ; les outliers de couverture sont
  tolérés jusqu'à 10 % des crops, au-delà le segment part en `review` ;
- un letterbox intermittent n'est signalé que si ≥ 10 % des crops ont des
  barres plus hautes (évite 2/99 et 5/53 sur 187) ;
- tout segment **keyframé** part en `review` : A1 ne déplace pas les clés ;
- 181 Concierge (« sans piège ») : 0 apply, 10 barres asymétriques en
  `review`, aucune action automatique.
- rendu 268 : barres brutes 194/170 px → 0/0 après A1 à 2,5, 5, 9, 24,5,
  35 et 54 s ; rendu 191 : plus de barre pleine largeur à 3, 20, 60, 120 et
  150 s. Les 15 px sombres en bas sont du contenu, pas une barre (bas actif
  calculé à 1958 px).

### 5.3 A2 — Overlays et sous-titres

**Statut mesuré (2026-10-27)** : A2 v2 applique la bannière rectangulaire
claire (187 DISNEYWORLD/ECOLE, 206,6 % vs 207 %, 0,4 %) ; le décor statique
de 187 ECOLE n'est plus confondu ; les logos de coin (191) et le logo de
template (295, `sdm.png`) vont en `review` ou relèvent de A6. 268 et 181
Concierge sont en `noop`. Le classifieur est durci sur le haut, le sombre,
la géométrie animée et les bandes partielles.

**Règles v2** :

- scan de **toute la hauteur** ; une bande haute plausible (24–200 px,
  largeur ≥ 35 %, persistance ≥ 60 %, ratio ≥ 3) bloque l'auto et part en
  `review` ;
- bande basse appliquée seulement si largeur ≥ 50 %, hauteur 24–200 px,
  persistance ≥ 60 %, luminosité médiane ≥ 60 et géométrie stable ;
- **sombre** : paires de bords horizontaux dans la médiane, médiane < 60 →
  `review` (détecte un rectangle sombre que le seuil de luminosité seul
  rate) ;
- **animée** : écart-type du centre et de la hauteur des bords > 12 px →
  `review` ; une bannière qui change seulement de luminosité ou de texte
  reste applicable si sa géométrie est stable ;
- **logo** : largeur < 35 % → `review` ; **bande partielle** : 35–50 % →
  `review` ; **décor** : hauteur > 200 px → `review` ;
- si un segment est keyframé, A2 ne modifie pas les clés : `review`.

Validation synthétique (`03-scenario-a/a2_synth.py`, 11 cas) : bannière
claire et décentrée → apply ; haute, sombre, glissante, logo, partielle,
décor, haut+bas → review ; aucune bande → noop ; animée en luminosité →
apply. Détail : `06-corpus/a5v2/RAPPORT-A5-V2.md`.

Deux niveaux :

1. **Bande persistante** : médiane temporelle sur N frames, ou template
   d'une zone basse ; si la bande est présente sur > 80 % des frames,
   calculer le scale d'exclusion (calage haut) :

   ```text
   scale >= 1920 / (1080 - hauteur_bande) * 100
   ```

2. **Sous-titre local** : OCR (Tesseract ou équivalent) sur la bande basse ;
   si le texte est présent sur une plage temporelle, produire un candidat
   `zoom_to_exclude` avec la plage et la boîte. **Ne pas auto-appliquer** si
   le zoom requis > 250 % ou si le visage sort du cadre.

Sortie : `source_overlays[]` dans `post.json`, avec `action` =
`exclude_global`, `zoom_local` ou `review`. A2 ne place jamais un logo
d'émission.

### 5.4 A3 — Keyframes

**Statut mesuré (2026-10-27)** : **invalidé en auto** sur 1059 segments
(précision 0,20, rappel 0,23 ; 35 faux positifs sur les vidéos sans piège).
A3 ne s'applique plus par défaut : il produit des **candidats** dans
`review.json`, ou s'active par flag explicite par projet après validation
humaine. Le protocole ci-dessous reste la spécification du candidat, pas une
autorisation d'écriture.

Entrées : `_ae.json` (`dataByFrame`, `segment_recommendations`), et si
disponible un artefact `_motion.json` avec X/Y.

Algorithme :

1. sélectionner l'objet recommandé par segment ;
2. extraire la trajectoire `(t, x)` (et `y` si disponible) ;
3. lisser avec One-Euro ;
4. simplifier avec RDP (tolérance 8–15 px) ;
5. garder 2–6 clés, en évitant les extrémités de coupe ;
6. convertir en `Position` avec l'ancre STEP6 et l'échelle cible ;
7. clamper à la couverture ;
8. appliquer avec `add_keyframe` (`interpolation: "bezier"` ou `easyEase`).

Critères :

- ancre inchangée ;
- 2–6 clés (tolérance 7 pour les cas complexes) ;
- pas de clé à moins de 2 frames d'une autre ;
- pas de clé sur une coupe ;
- RMSE vs humain ≤ 20 px sur les frames échantillonnées.

### 5.5 A4 — Transitions

**Statut mesuré (2026-10-27)** : validé sur le mécanique (fusion < 0,5 s,
cut dur manqué) ; les splits sémantiques (KATIA, corrélation 0,99) restent en
`review`. À mesurer sur le corpus élargi avant auto.

Signaux :

- perte de l'objet recommandé sur N frames ;
- réacquisition avec un id différent ;
- saut de position > seuil (ex. 150 px en 1 frame) ;
- incohérence entre le CSV et le mouvement.

Actions :

- `split` si perte/rupture nette ;
- `merge` si deux segments consécutifs ont un mouvement continu et un cut
  absent du CSV ;
- sinon `review`.

Appliquer via `edit.splitLayer` ou `layer.timing` sur une copie, puis vérifier
par frames.

### 5.6 A5 — Rapport et QC déterministe

**Statut mesuré (2026-10-27, v2)** : prototype exécuté sur 268, 187
DISNEYWORLD, 187 ECOLE, 191 cancer, 181 Concierge et 295 SDM. Les trois
artefacts sont produits sans modifier le projet ; A2 est à 0,4 % de l'humain
sur 187, A1 est dans la plage humaine mais pas à 2 % près (variation
créative), A3 reste en review. 181 Concierge (« sans piège ») : 0 action
automatique, 10 barres asymétriques et 8 candidats A3 en review. A5 v2 ajoute
`timings_ms` par étage (0,07–0,24× la durée vidéo, sans rendu) et un hash
vidéo **échantillonné** (début/milieu/fin, 3 × 4 Mio) au lieu du SHA-256
complet. A6 est intégré en option `--template sdm` (décision et bloc
`template` ; en production, l'écriture est faite par le module Rust `post`,
D-032). La détection de cut
dur intra-segment reste désactivée par défaut. Script :
`/tmp/opencode/step9-spike/03-scenario-a/a5_report.py` ; rapports :
`06-corpus/a5v2/RAPPORT-A5-V2.md` et `06-corpus/a5/RAPPORT-A5.md`.

Produire trois artefacts :

- `post.json` : actions appliquées (scale, keyframes, splits/merges) et
  confiance ;
- `review.json` : signaux sous le seuil, raisons, frames de preuve ;
- `audit.json` : contrôle du `.ecproj` produit — durée, nombre de segments,
  ancres, clés, échelles, overlays détectés — et, si nécessaire, quelques
  `render_frame` de contrôle. **Pas de rendu final.**

Checks :

- specs ffprobe ;
- frames noires/figées (`blackdetect`, `freezedetect`) ;
- audio (durée, silence) ;
- OCR des sous-titres ;
- détection de visage hors cadre (modèle léger ou STEP4) ;
- cohérence durée/nombre de segments.

### 5.7 A6 — Template SDM (validé sur 295)

Cas récurrent identifié sur 295 : une seule coupe source, dupliquée trois
fois (deux tailles + la même image floutée en fond), sur toute la durée. Ce
n'est pas un geste de cadrage : c'est un **template de composition**.

Spécification mesurée sur le GOOD 295 (2026-10-27). Le brut et le GOOD
partagent le même `_ae.json` et le même `_tracking.json` (SHA-256
identiques) : la seule différence est le `.ecproj`.

| Calque | Échelle | Position (x, y) | Rôle |
| --- | ---: | ---: | --- |
| Fond | 180 % | (540,095, 0) | source 9:16 floutée (Gaussien 50) |
| Principal | 84,3497774862575 % | (540,095, 171,032) | panneau 1080×911 |
| Secondaire | 56,294573830734535 % | (540,095, 1082,026) | panneau 16:9 1080×608 |
| Logo | 100 % | (540, 960) | `sdm.png` (optionnel) |

- déclencher le template par une règle de nommage **configurée**
  (`M6 - SDM - ` par défaut) ou un mapping projet, pas par une heuristique
  de contenu ; vérifier ensuite les conditions structurelles ;
- ordre d'affichage : logo, secondaire, principal, fond ;
- positions, échelles, rayon de flou et ordre des calques lus depuis un
  profil versionné, jamais devinés. Conserver les valeurs mesurées : un
  profil arrondi à 84,375 %/56,25 % et y=171/1082 dévie de 106/255 ;
- une seule coupe source, aucune keyframe, aucun parent animé. Un projet
  multi-segment ou keyframé part en `review` ;
- les animations sporadiques du plan large restent humaines ;
- si le template n'est pas configuré, A ne fait rien et écrit `review`.

Prototype `06-corpus/a6_template.py` : transformation JSON du `.ecproj` brut
(parent Null supprimé, ancre au centre source 960, position 540,095, flou et
duplicats, logo). Coût 0,03 s / 12 Mo. Rendu de contrôle à 0, 10, 30 et
50 s : différence maximale 0/255, moyenne 0, 0 pixel > 2. A6 est donc
validé au niveau projet et rendu.

**Intégration v2 (2026-10-27)** : `03-scenario-a/a6_apply.py` écrit le projet
transformé (`--apply`) et les trois artefacts additifs ; `a5_report.py
--template sdm` ajoute seulement la décision A6 et le bloc `template` à son
rapport. Résultats : 295 → A6 apply, audit `pass`, rendu relinké 0/255 ;
295 avec une keyframe → A6 review, aucun projet ; 187 → A6 noop. Le
déclencheur reste le préfixe **configuré** `M6 - SDM - ` (ou un mapping
projet), pas une inférence de contenu ; le nom de la vidéo est le signal
pratique. Ce module est branché dans le post-traitement de production depuis
D-032 (voir §14.1), pas dans le panel.

### 5.8 Hors périmètre A, par décision

- **relink** : contrôle optionnel `--check-media` ; les chemins sont stables
  sur le même poste, donc pas de module ;
- **logos/jingle** : jamais ajoutés automatiquement ; les PNG restent
  importés mais non ajoutés à la comp, les `.mov` restent ignorés, les
  directives client sont humaines. Seule exception : le logo fixe d'un
  template A6 explicitement configuré (`sdm.png`), fourni par le profil ;
- **rendu H.264** : l'export EffectCraft CPU n'est pas optimal et le GPU
  H.264 n'est pas disponible ; l'export final reste humain, avec l'outil GPU
  externe. A s'arrête au `.ecproj` + rapport.

### 5.9 Livrables A

- `post.json`, `review.json`, `audit.json` ;
- `.ecproj` modifié (copie) ;
- `rapport-A.md` avec mesures, écarts vs GOOD, temps ;
- **aucun MP4** : le rendu est hors périmètre.

## 6. Phase B — Agent QA read-only

### 6.1 Jeu de labels

Sélectionner 100–200 frames :

- 1 frame au début, au milieu et à la fin de chaque segment ;
- 1 frame autour des cuts ;
- 1 frame autour des splits/zooms spéciaux ;
- quelques frames négatives (pas de logo/sous-titre).

Labels manuels :

| Champ | Valeurs |
| --- | --- |
| `logo_present` | `true` / `false` |
| `logo_bbox` | `[x, y, w, h]` ou `null` |
| `subtitle_present` | `true` / `false` |
| `subtitle_bbox` | `[x, y, w, h]` ou `null` |
| `banner_present` | `true` / `false` |
| `face_out_of_frame` | `true` / `false` |
| `black_bars` | `true` / `false` |
| `notes` | texte libre |

### 6.2 Checks déterministes

- ffprobe ;
- `blackdetect` / `freezedetect` ;
- template matching des logos connus ;
- OCR de la bande basse ;
- détection de visage (modèle léger) ;
- continuité des cuts.

### 6.3 Checks VLM

Prompt structuré, sortie JSON :

```json
{
  "logo_present": false,
  "subtitle_present": false,
  "banner_present": false,
  "face_out_of_frame": false,
  "framing_issue": false,
  "confidence": 0.0,
  "notes": "..."
}
```

Règles :

- images downscalées (max 960 px) ;
- 1 appel par frame ou par lot de 3–5 frames ;
- température 0 ;
- schéma JSON strict ;
- pas de texte libre dans la décision ;
- journaliser modèle, version, prompt hash.

### 6.4 Modes

| Mode | Avantage | Inconvénient |
| --- | --- | --- |
| Local 3B Q4 | offline, gratuit | qualité limitée, VRAM 4 Go |
| Cloud | qualité supérieure | réseau, coût, confidentialité |
| Hybride | local pour le facile, cloud pour le doute | complexité |

Commencer par le local ; si le rappel est insuffisant, tester le cloud sur un
sous-ensemble avec accord explicite.

### 6.5 Livrables B

- `audit.json` ;
- `labels.json` ;
- `rapport-B.md` avec précision/rappel, faux positifs, coût, latence ;
- matrice de confusion par type de défaut.

## 7. Phase C — Co-monteur (laboratoire)

### 7.1 Périmètre

3–5 segments choisis parmi les cas sémantiques :

- 298 segment 21 : split à 53,64 s ;
- JOSEPH segment 5 : zoom 254,7 % pour sous-titre ;
- KATIA segment 7 : zoom 287,6 % pour archive 4:3 ;
- KATIA segments 26/49 : splits à 82,6 s et 176,64 s.

### 7.2 Boucle

```text
open_project(copie)
  → get_comp / get_layer
  → render_frame(début, milieu, fin)
  → VLM : intention + confiance
  → exécuteur : set_property / add_keyframe / split
  → render_frame(vérification)
  → comparaison baseline
  → accept / undo
  → max 3 itérations
  → save + revue humaine
```

### 7.3 Garde-fous

- copie uniquement ;
- `--automation-write-root` limité à la copie ;
- max 3 itérations par segment ;
- deltas bornés ;
- pas de clé sur une coupe ;
- `batch` atomique ;
- `undo` si régression ;
- approbation humaine avant `save` ; le rendu reste l'outil GPU humain.

### 7.4 Mesures

- préférence aveugle A vs C (5 juges, 5-point Likert) ;
- écart d'édition vs humain (nombre de gestes, distance de position) ;
- temps ;
- coût ;
- taux d'échec/rollback.

### 7.5 Livrables C

- `rapport-C.md` ;
- `decisions.json` (journal des intentions) ;
- projet C, frames de preuve, comparaison A/C ;
- recommandation go/no-go.

## 8. Métriques et portes de décision

### 8.1 Tableau de bord

| Métrique | A | B | C |
| --- | --- | --- | --- |
| Ancre vs STEP6 | 0 px | — | 0 px |
| Keyframes | 2–6 (tolérance 7) | — | 2–6 (tolérance 7) |
| Écart position vs humain | strict ≤ 20 px ; utile ≤ 60 px, sinon `review` | — | idem |
| Scale | ≤ 2 % | — | ≤ 2 % |
| Détection overlay | précision ≥ 95 %, rappel ≥ 90 % | — | — |
| Zéro action sur vidéo propre | 100 % | — | — |
| Rendu final | hors périmètre | contrôle si humain | hors périmètre |
| Précision logo | — | ≥ 95 % | — |
| Rappel logo | — | ≥ 90 % | — |
| Précision sous-titre | — | ≥ 95 % | — |
| Rappel sous-titre | — | ≥ 85 % | — |
| Visage hors cadre | — | ≥ 95 % | — |
| Faux positifs | — | ≤ 10 % | — |
| Temps/vidéo | ≤ 3 min (sans rendu) | ≤ 5 min | ≤ 15 min |
| Coût/vidéo | ~0 | ≤ 0,50 € | ≤ 2 € |
| Déterminisme | hash stable | rapport variable | projet variable |

**Mesures du corpus élargi (2026-10-27)** :

| Métrique | Cible | Mesuré |
| --- | --- | ---: |
| Ancre vs STEP6 | 0 px | 0 px sur 298/300 ; non attribuable sur le corpus élargi (dérive de baseline) |
| Clés ≤ 60 px d'un objet suivi | ≥ 80 % | 278/520 (53,5 %) |
| Précision A3 (règle amplitude) | ≥ 95 % | 34/172 (0,20) |
| Faux positifs A3 sur « sans piège » | 0 | 35 segments |
| Segments statiques déjà bien cadrés | — | 458/907 (50,5 %) |
| Segments statiques à re-cadrer (> 100 px) | — | 363/907 (40,0 %) |
| Scale A1 (191) | ≤ 2 % | 249,6 % vs 255 % (2,1 %) avec marge 4 % ; 255 % avec marge 6,25 % |
| Scale A2 (187/300) | ≤ 2 % | 207,4 % vs 207 % (0,4 %) ; 187 ECOLE corrigé à 206,6 % par le détecteur A5 |
| A1 couverture après scale (268/191) | 100 % | 15/15 et 37/37 applies couverts ; `top_min −38,4`, `bottom_max 1958,4` |
| A1 sur « sans piège » (181) | 0 apply | 0 apply, 10 barres asymétriques en review |
| A2 sur « sans piège » (181) | 0 apply | 0 apply, noop |
| A2 validation synthétique | 11/11 | 3 apply, 7 review, 1 noop conformes |
| A5 temps/vidéo (sans rendu) | ≤ 3 min | 3,7–33,5 s, soit 0,07–0,24× la durée |
| A5 hash vidéo | stable | échantillon 3 × 4 Mio + taille, offsets inclus |
| A6 intégration | 295 apply / keyframé review | 295 apply + audit pass ; 295 keyframé review ; 187 noop |

Le 40 % de segments statiques à re-cadrer inclut la dérive de tracking : avec
le tracking legacy, 181 Concierge passe de 25/58 à 12/58 et 187 DISNEYWORLD
reste à 16/28. C'est une **borne haute**, pas une mesure purement humaine.

### 8.2 Portes

- **G1** : H1 validée → intégration optionnelle de A.
- **G2** : H2 validée → pilote B.
- **G3** : H3 validée → pilote C limité.
- **Kill** : une hypothèse échoue ou la gouvernance des données n'est pas
  validée → arrêt.

### 8.3 Taille de l'échantillon

Le spike initial porte sur 2 projets et 120 segments. Le corpus élargi du
2026-10-27 atteint **19 paires / 32 vidéos exploitables** (38 vidéos au
total, 6 sans référence fine), ce qui satisfait le volume cible. En revanche,
la strate « contrôles propres » est plus faible que prévu : les 5 paires
« sans piège » (181, 270, 276, 286, 287) contiennent encore 13 segments
keyframés, 40 clés et 62 deltas d'ancre. Il n'existe donc pas, dans ce
corpus, de vidéo où le monteur ne touche à rien. La mesure des faux positifs
doit utiliser ces paires comme **contrôles à gestes faibles**, pas comme
vérité terrain absolue.

Pour une décision de production, viser **10–15 projets / 20–30 vidéos**,
stratifiés (pas un échantillon aléatoire) :

| Strates | Minimum | Ce que cela teste |
| --- | ---: | --- |
| **Contrôles propres** (aucun piège) | 5–8 vidéos | le taux de faux positifs : A doit ne rien faire |
| **Letterbox** (2.39:1, 4:3, pillarbox) | 3–5 | A1, calcul de la zone active |
| **Overlays** (logo fixe, logo mobile, bannière, ticker) | 3–5 | A2, persistance et seuils |
| **Sous-titres** (bande basse, sous-titres créatifs) | 3–5 | A2, OCR et abstention |
| **Mouvement / groupe** (plan long, sujet qui sort) | 3–5 | A3, keyframes et couverture |
| **Transitions** (fondu, cut manqué, faux cut) | 3–5 | A4, split/merge |
| **Archive / 4:3** | 2–3 | A2/A1, cas mixte |
| **Logos/jingle** (hors A) | 2–3 | non-régression du panel, pas une cible A |

Si seulement 6–8 paires sont disponibles, prioriser : **2–3 contrôles propres**,
puis une paire par piège principal. Les contrôles propres sont les plus
importants : ils mesurent le risque de casser une vidéo qui n'avait besoin de
rien.

Pour chaque projet, fournir une fiche :

- nom du projet, fichiers source, format, durée ;
- pièges observés (letterbox, logo, sous-titre, mouvement, cut) ;
- gestes humains effectués (échelle, keyframes, split/merge) ;
- directives client (logo/jingle) et emplacement des assets ;
- projet `_GOOD` correspondant, si disponible ;
- notes particulières (visages, groupe, archive, langue).

Les paires **brut + GOOD** sont nécessaires pour mesurer l'écart. Un brut seul
permet de tester la détection, pas la qualité de la reproduction. Les
résultats du spike restent une **faisabilité**, pas une validation statistique.

## 9. Commandes utiles

### 9.1 Inspection d'un projet

```sh
effectcraft-cli --project projet.ecproj info
effectcraft-cli --project projet.ecproj props "The 100 extrait 2_9x16" 82 --flat
effectcraft-cli --project projet.ecproj get "The 100 extrait 2_9x16" 82 transform/position --time 5
```

### 9.2 Rendu de frames

```sh
effectcraft-cli --project projet.ecproj \
  render-frame --time 5 --max-side 960 --out frame.png
```

### 9.3 MCP

```sh
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"spike","version":"1"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
| effectcraft-cli mcp --automation-read-root "$ROOT" \
                       --automation-write-root "$ROOT/03-scenario-c"
```

### 9.4 Rendu final (hors périmètre A, référence pour l'export humain)

```sh
effectcraft-cli --project projet.ecproj \
  render --out sortie_9x16.mp4 --format h264 --rate-control bitrate --bitrate 10000 --progress
```

Cette commande sert de **référence de contrôle** ou de baseline B ; elle n'est
pas une sortie du prototype A. En production, l'export reste l'outil GPU
externe, avec validation humaine.

### 9.5 Contrôle

```sh
ffprobe -v error \
  -show_entries stream=width,height,r_frame_rate \
  -show_entries format=duration,bit_rate,size \
  -of default=nw=1 sortie_9x16.mp4
```

## 10. Critères d'arrêt

Arrêter immédiatement si :

- une écriture sort de `/tmp/opencode/step9-spike` ;
- `/mnt/cache` ou `runtime/` est modifié ;
- un projet `_GOOD` est écrasé ;
- un test existant ou un contrat JSON/CSV régresse ;
- A produit deux hash différents pour les mêmes entrées ;
- un export est livré sans revue humaine ;
- des frames sont envoyées au cloud sans accord écrit ;
- le temps ou le coût dépasse le budget du spike.

## 11. Livrables de fin de spike

1. `docs/etudes/ETUDE-STEP9-AGENTIQUE.md` — étude (ce document associé).
2. `docs/etudes/SPIKE-STEP9-PROTOCOLE.md` — protocole.
3. `rapport-A.md`, `rapport-B.md`, `rapport-C.md` — résultats.
4. `labels-*.json`, `audit.json`, `post.json` — données reproductibles.
5. `memory-bank/decisionLog.md` — D-029 et mise à jour éventuelle.
6. `memory-bank/activeContext.md` — état de la session.
7. Recommandation go/no-go pour chaque scénario.

## 12. Calendrier indicatif

| Semaine | Travail | Sortie |
| --- | --- | --- |
| S1 | baseline, labels, mesures — **fait le 2026-10-26** | `01-baseline`, `02-labels`, `RAPPORT-PHASE0.md` |
| S2–S3 | A0–A3 (profilage, letterbox, overlays, keyframes) ; A2 durci, A3 en review (D-030) | prototype A partiel |
| S4 | A4–A5 (transitions, rapport/QC) ; A5 exécuté sur 4 cas | `RAPPORT-A5.md` |
| S5–S6 | B (labels, checks, VLM local/cloud) | `rapport-B.md` |
| S7 | C (3–5 segments, revue humaine) | `rapport-C.md` |
| S8 | synthèse, décision, docs | D-029 mise à jour |

Le calendrier est indicatif ; il suppose une personne à temps partiel et ne
comprend pas les dépendances matérielles (GPU, modèles).

## 13. Gouvernance des données

- Les frames peuvent contenir des visages, des voix, des lieux privés.
- Le mode local est le défaut ; le cloud est opt-in par projet.
- Un envoi cloud exige : base légale, DPA, région, durée de rétention,
  suppression après traitement, journal d'accès.
- Ne jamais envoyer un projet client sans accord écrit.
- Journaliser : projet, frames envoyées, modèle, version, hash du prompt,
  date, destinataire.
- Préférer des frames downscalées et anonymisées quand c'est possible.

## 14. Ce que le spike ne fait pas

- il ne modifie pas les 8 étapes ;
- il ne crée pas de 9e step serveur ;
- il ne change pas `_ae.json`, `_audio.json`, le CSV ou le panel ;
- il ne valide pas la production ;
- il ne remplace pas la revue humaine ;
- il ne garantit pas le « zéro logo » absolu.

### 14.1 Industrialisation A5/A6 (2026-10-27)

Le périmètre A5/A6 n'est plus un spike : il est porté dans
`crates/workflow-steps/src/post/`, appelé après un STEP8 réussi et exposé par
`workflow-cli post`. Le brut reste intact, un projet `_assist.ecproj` est
toujours publié, et les artefacts `post.json`/`review.json`/`audit.json` sont
additifs. A3/A4 restent en revue, A1/A2 ne déplacent pas l'Anchor Point, A6
applique le template SDM mesuré. Le détail, les validations et les limites
sont dans `docs/lots/LOT-9-POST-STEP8.md` et la décision **D-032**.

Les scénarios B (QA agentique) et C (co-monteur) restent hors production :
ce protocole conserve leur cadre d'expérimentation.

## 15. Conclusion attendue

À la fin du spike, on doit pouvoir répondre par oui/non, avec des chiffres :

1. A couvre-t-il ≥ 70 % du delta humain, de façon déterministe ?
2. B atteint-il précision ≥ 95 % et rappel ≥ 90 % sur les défauts ?
3. C apporte-t-il une préférence ≥ 60 % sans régression ?

Si les trois réponses sont non, la conclusion est simple : **le pipeline n'a
pas besoin d'un agent co-monteur ; il a besoin d'un meilleur exécuteur
déterministe et d'un auditeur**. C'est l'hypothèse la plus probable au vu du
corpus 298/300.
