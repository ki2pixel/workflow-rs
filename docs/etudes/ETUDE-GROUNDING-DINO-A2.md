# Grounding DINO pour A2 et les logos source — spike hors production

- **Type** : étude + spike
- **Statut** : Preuve datée, aucun code de production modifié
- **Date** : 2026-11-09
- **Portée** : A2 du post-STEP8, registre `assets/models/qa/source_logos.toml`,
  contrôle QA des logos
- **Décision** : D-060 (laboratoire seulement, pas d'intégration A2)
- **Voir aussi** : D-051, D-052, D-055, D-056, D-058, D-059 (réservé A1),
  `docs/lots/LOT-9-POST-STEP8.md`, `docs/lots/LOT-10-QA.md`
- **Preuves** : `docs/etudes/ETUDE-GROUNDING-DINO-A2/results/`
- **Script** : `scripts/spike-grounding-dino.py`

## 1. Résumé exécutif

La question était double : Grounding DINO peut-il (a) retrouver un logo de
programme absent du catalogue et (b) confirmer un bandeau bas quand l'OCR
échoue, avec une persistance temporelle qui élimine les faux positifs ? Le
spike répond **oui pour la découverte hors ligne, non pour une intégration A2
en production**.

Mesures principales, modèle `IDEA-Research/grounding-dino-tiny` (Swin-T,
172,3 M paramètres, 659 Mio), 242 frames, prompt composite
`logo . text . watermark . lower third . banner`, seuil 0,20–0,25 :

1. **Localisation des logos connus** : 118/122 frames du catalogue à
   IoU ≥ 0,50 au seuil 0,20 (96,7 %, IC 95 % 91,9–98,7 %) ; 112/122 au seuil
   0,25 (91,8 %, IC 95 % 85,6–95,5 %). L'IoU médian est de 0,90 sur ces
   122 frames.
2. **Bandeaux bas A2** : 59/60 frames à 0,20 (98,3 %) et 55/60 à 0,25
   (91,7 %) ; l'IoU texte médian est de 0,66 à 0,20 (0,64 à 0,25) sur les
   36 frames où la vérité terrain textuelle existe. Deux styles seulement
   sont couverts (UJUD, 66 Minutes).
3. **Faux positifs** : au niveau frame, 25 % des 60 frames négatives à 0,20
   et 20 % à 0,25. Après clustering temporel à 70 %, **0/5 clips négatifs**
   ne produit de cluster persistant (persistance maximale observée 16,7 %).
   Le filtre de persistance fait donc son travail sur cet échantillon, mais
   cinq clips ne suffisent pas à certifier un taux ≤ 5 %.
4. **Persistance** : 5/5 clips positifs ont un logo persistant ≥ 70 % à 0,20
   (100, 100, 100, 100, 91,7 %) ; 3/5 clips ont un texte persistant ≥ 70 %
   (54 et 58 à 100/91,7 %, 40 CITROUILLES à 83,3 % ; 40 ABHARTACH et
   40 FOOTBALLEUR restent à 66,7 % et 50 %, ce qui est cohérent avec leurs
   bandeaux animés déjà en `review`).
5. **Coût local** : 1,279 s/frame à 480 px, 1,47 Gio de VRAM, 3,48 s de
   chargement du modèle. Douze frames coûtent 15,3 s à chaud (18,8 s avec
   chargement), six frames 7,7 s (11,1 s). A2 OCR coûte 13,0 s et le post
   complet 43,9 s : l'ajout à 12 frames porte le post à ~59 s (+35 %).
6. **Cloud** : Replicate n'a **pas été exécuté** faute de
   `REPLICATE_API_TOKEN`. La page publique donne ~0,00098 $/run, ~1 s sur
   L40S, soit ~0,012 $ pour 12 frames. Le coût n'est pas le frein ; l'egress
   de rushes client, le réseau et le secret le sont.

**Décision D-060** : Grounding DINO reste un outil de **découverte et
d'annotation hors ligne** pour proposer des entrées au catalogue
`source_logos.toml` et qualifier les revues A2. Aucun flag de production,
aucune modification de `PostConfig`, `a2.rs` ou `config/config.toml`. Le
backend local PyTorch 480 px est retenu pour le laboratoire ; Replicate est
écarté pour la production ; ONNX n'est pas prêt (pas de config d'export
Optimum, ops de deformable attention, text encoder BERT).

## 2. Besoin et critères d'acceptation

### 2.1 Limites actuelles

- `source_logos.toml` couvre 30 émissions, 21 clean feeds et 9 émissions sans
  logo. Un logo inconnu, un nouveau programme ou une variante non cataloguée
  échappe au prior spatial ; A2 ne peut pas l'identifier et le traite comme
  une bande étroite ou un décor.
- La détection géométrique de bandeaux produit parfois des candidats larges
  qui incluent le logo et le texte ; l'OCR PP-OCRv6 confirme le texte quand il
  est lisible, mais échoue sur les polices stylisées, les bandeaux sombres et
  les incrustations dégradées.
- DINOv2 (D-048/D-049) confirme des logos **texte** haute bande, mais reste un
  probe fermé entraîné sur 13 assets synthétiques ; il ne fait pas de
  détection ouverte.

### 2.2 Critères définis avant le spike

| Critère | Cible | Mesure |
| --- | --- | --- |
| Rappel logo | ≥ 90 % à IoU ≥ 0,50 | 122 frames du catalogue |
| Précision spatiale | centre dans la bbox GT ≥ 95 % | 122 frames |
| Faux positifs frame | ≤ 5 % sur clean feeds | 60 frames négatives |
| Faux positifs clip | 0 cluster persistant ≥ 70 % | 5 clips négatifs |
| Bandeau bas | rappel bande ≥ 90 % | 60 frames A2 |
| Texte | IoU ≥ 0,30 sur ≥ 80 % | 36 frames avec texte GT |
| Persistance logo | ≥ 70 % sur 12 frames | 5 clips positifs |
| Persistance texte | ≥ 70 % sur 12 frames | 5 clips positifs |
| Latence | ≤ 8 s / 6 frames ou ≤ 16 s / 12 frames | GTX 1650 4 Gio |
| VRAM | ≤ 2,5 Gio | GTX 1650 4 Gio |
| Sécurité | aucun faux `apply` | revue de toutes les décisions |

Le dernier critère est le plus important : dans la version évaluée,
Grounding DINO ne peut **jamais** créer un `apply` ni retirer un veto. Il
annote, il ne décide pas.

## 3. Protocole

### 3.1 Corpus

Le manifeste contient 242 frames, toutes lues en lecture seule :

| Source | Frames | Détail |
| --- | ---: | --- |
| `reference_logo` | 122 | 30 émissions du catalogue, captures client 1920×1080, 1 à 12 frames par émission |
| `a2_lower_third` | 60 | 5 clips × 12 frames aux instants du gate OCR A2 : 54 UJUD, 58 UJUD, 40 66M CITROUILLES/ABHARTACH/FOOTBALLEUR |
| `negative` | 60 | 5 clean feeds × 12 frames : WANTED, Pretty Little Liars, Younger, Le Seigneur des Anneaux, 6ter La petite maison dans la prairie |

Les vérités terrain logos viennent des `bbox` et `variants` de
`source_logos.toml` ; les vérités terrain bande/texte viennent des
`post.json` A2 réels (`details.bbox` et `details.ocr.bbox`). Les frames des
clips A2 sont extraites aux 12 instants `duration × (i + 0,5) / 12`, comme
`A2_OCR_SAMPLES`.

Limites du corpus : les 122 captures sont **curées** (elles montrent le logo),
elles ne mesurent pas le rappel sur des frames quelconques ; les 30 émissions
ne sont pas indépendantes (variantes d'une même identité visuelle) ; les cinq
clips négatifs ne contiennent pas de faux positif statique volontaire ; deux
styles de bandeau seulement sont couverts. Le spike est une preuve de
faisabilité, pas une validation de production.

### 3.2 Prompts comparés

Sept prompts ont été comparés sur 50 frames (30 logos, 10 A2, 10 négatifs) :

- `logo`
- `text`
- `logo . text`
- `lower third`
- `television logo`
- `banner`
- `logo . text . watermark . lower third . banner` (composite, retenu)

Le wrapper Replicate attend des requêtes séparées par des **virgules** ; le
script convertit `logo . text` en `logo, text` pour ce backend. Le modèle
transformers utilise les points.

### 3.3 Filtrage spatial

Deux branches séparées, jamais un mélange :

- **logo** : label contenant `logo`, largeur ≤ 0,35 × largeur source (le même
  `LOGO_MAX_WIDTH` que A2), centre dans la zone bas-gauche
  (`x ≤ 0,40`, `y ≥ 0,55`) ou haut-gauche (`x ≤ 0,40`, `y ≤ 0,30`).
- **texte/bandeau** : label contenant `text`, `lower`, `banner`, `third` ou
  `subtitle`, largeur ≥ 0,20 × largeur, centre `y ≥ 0,60`.

Les boîtes de surface > 60 % de la frame, de largeur > 0,80 ou de hauteur
< 2 px sont rejetées. Le filtre de largeur 0,35 est indispensable : sans lui,
le prompt composite produit des boîtes « logo + bandeau » de 50–70 % de
largeur qui confondent le logo et le texte. Avec lui, la branche logo garde
les boîtes serrées et la branche texte garde les bandeaux.

### 3.4 Persistance temporelle

Clustering glouton par frame :

- IoU ≥ 0,50 avec la première détection du cluster ;
- distance des centres ≤ 10 px (source 1080p) ;
- persistance = frames du cluster / frames échantillonnées ;
- persistant si ≥ 70 % (9/12 ou 5/6).

Le clustering est appliqué séparément aux branches logo et texte. Il est
volontairement simple : pas de tracker, pas de modèle de mouvement, pas de
moyenne de bloc. Une détection isolée ne peut jamais devenir persistante.

### 3.5 Sécurité de l'authentification

Le script ne lit **que** `REPLICATE_API_TOKEN` dans l'environnement. Il
n'écrit jamais le jeton dans le manifeste, les résultats, les logs ou le
dépôt. Le backend Replicate n'est instancié que si le jeton existe ; sans
jeton, il échoue avant tout appel réseau. Le modèle local ne fait aucun appel
réseau après le téléchargement des poids.

## 4. Résultats

### 4.1 Comparaison de prompts

Sur les 50 frames de la phase comparative, seuil 0,20 :

| Prompt | Rappel logo IoU≥0,5 | Rappel bande | IoU texte médian | FP frame (10 négatifs) |
| --- | ---: | ---: | ---: | ---: |
| `logo` | 0,0 % | 0,0 % | 0,000 | 0 % |
| `text` | 0,0 % | 0,0 % | 0,000 | 0 % |
| `logo . text` | 90,0 % | 30,0 % | 0,291 | 50 % |
| `lower third` | 0,0 % | 0,0 % | 0,000 | 0 % |
| `television logo` | 0,0 % | 0,0 % | 0,000 | 0 % |
| `banner` | 0,0 % | 0,0 % | 0,000 | 0 % |
| composite | **97,5 %** | **100,0 %** | **0,659** | 40 % |

Constat contre-intuitif : les prompts à **un seul terme** sont quasiment
inopérants au seuil 0,20, alors que `logo . text` et le composite trouvent le
logo. La similarité texte-image de Grounding DINO dépend de la séquence
complète ; un mot isolé ne suffit pas. Le composite est retenu pour la suite.

Le label tokenizer fusionne parfois `text` et `watermark` en `textmark` ; le
code doit donc tester des **sous-chaînes**, pas des labels exacts.

### 4.2 Validation complète (242 frames)

Prompt composite, backend local, 480 px, seuil balayé hors ligne :

| Seuil | Rappel logo (122) | Centre GT | IoU médian (122) | Rappel bande (60) | Texte IoU≥0,3 (36) | FP frame logo (60) | Clips négatifs persistants |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0,15 | 99,2 % | 99,2 % | 0,901 | 100,0 % | 100,0 % | 33,3 % | 0/5 |
| 0,20 | 96,7 % | 96,7 % | 0,897 | 98,3 % | 97,2 % | 25,0 % | 0/5 |
| 0,25 | 91,8 % | 92,6 % | 0,893 | 91,7 % | 77,8 % | 20,0 % | 0/5 |
| 0,30 | 85,2 % | 85,2 % | 0,882 | 68,3 % | 47,2 % | 15,0 % | 0/5 |
| 0,35 | 75,4 % | 75,4 % | 0,873 | 16,7 % | 13,9 % | 8,3 % | 0/5 |

Les colonnes logo (rappel, centre, IoU médian) portent sur les 122 frames
`reference_logo`. Le rapport brut `results.md` agrège les 182 frames avec
logo (122 références + 60 A2) : il affiche 97,2 % au seuil 0,20 et un IoU
médian de 0,883. Les deux lectures sont cohérentes, mais seule la première
mesure le catalogue.

Lecture :

- **0,20** est le meilleur compromis rappel/précision pour la découverte :
  96,7 % de logos, 98,3 % de bandeaux, 97,2 % de texte, et 0/5 faux
  positifs persistants. Le taux de faux positifs frame reste toutefois à
  25 %, trop haut pour un gate automatique.
- **0,25** est plus conservateur : 91,8 % de logos, 91,7 % de bandeaux,
  mais le texte tombe à 77,8 %.
- Aucun seuil ne respecte le critère ≤ 5 % de faux positifs **par frame**.
  Le critère « 0 cluster persistant » est respecté sur les cinq clips, mais
  l'intervalle de confiance à 95 % de 0/5 est [0 ; 43,4 %] : l'échantillon ne
  prouve pas un taux ≤ 5 %.

### 4.3 Détail par source et échecs

Au seuil 0,25 :

| Source | Frames | Rappel logo | Centre | IoU médian | Rappel bande | Texte IoU≥0,3 | FP frame |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `reference_logo` | 122 | 91,8 % | 92,6 % | 0,893 | — | — | — |
| `a2_lower_third` | 60 | 91,7 % | 91,7 % | 0,715 | 91,7 % | 77,8 % | — |
| `negative` | 60 | — | — | — | — | — | 20,0 % |

Échecs de localisation sur le catalogue (10/122 à 0,25, 4/122 à 0,20) :

- **Enquête d'action** (3/4 à 0,25) : la boîte détectée couvre le logo **et**
  le bandeau `[101, 923, 1022, 1031]` au lieu du seul logo
  `[101, 918, 282, 1021]` ; l'IoU tombe à 0,18. À 0,20, une boîte serrée
  `[107, 924, 279, 1018]` à 0,245 récupère la frame.
- **Enquête Exclusive** (2/2 à 0,25) : même confusion logo + bandeau
  `[64, 919, 1321, 1020]` contre `[61, 913, 389, 1021]`, IoU 0,25.
- **Zone Interdite**, **Les Traîtres** (1/4 chacun à 0,25) : boîte de bande
  au lieu du logo.
- **Capital** (1/2) : boîte serrée sur un sous-élément, IoU 0,369 mais
  centre dans le logo ; le filtre de persistance la garde.
- **Top Chef** (1/2), **Piquantes !** (1/4) : aucune détection au seuil ;
  logo petit/transparent ou haut-gauche.
- **Enquête d'action** frame 3 et **Top Chef** frame 1 restent sans
  détection même à 0,20.

La confusion logo/bandeau est le principal défaut pour A2 : le modèle
préfère souvent une boîte contextuelle « logo + lower third » à une boîte
serrée. La branche logo avec largeur ≤ 0,35 et le prompt composite corrigent
une partie du problème, mais pas tous les cas. C'est une raison
supplémentaire de ne pas laisser Grounding DINO décider seul.

### 4.4 Persistance temporelle

Au seuil 0,20, 12 frames par clip :

| Clip | Logo | Texte | Logo 6 frames | Texte 6 frames |
| --- | ---: | ---: | ---: | ---: |
| 40 66M ABHARTACH | 100,0 % | 66,7 % | 100,0 % | 100,0 % |
| 40 66M CITROUILLES | 100,0 % | 83,3 % | 100,0 % | 100,0 % |
| 40 66M FOOTBALLEUR | 100,0 % | 50,0 % | 100,0 % | 100,0 % |
| 54 UJUD HOTEL | 100,0 % | 100,0 % | 100,0 % | 100,0 % |
| 58 UJUD CROUSTY | 91,7 % | 91,7 % | 100,0 % | 100,0 % |

Le sous-échantillonnage à 6 frames (indices pairs) donne 100 % partout,
y compris sur ABHARTACH/FOOTBALLEUR où le texte n'est présent que sur 50–67 %
des 12 frames. **Conclusion : 6 frames suffisent pour le logo, pas pour
juger la persistance d'un texte intermittent.** Le gate A2 utilise déjà 12
frames ; Grounding DINO doit utiliser les mêmes 12 frames, pas 6, s'il doit
confirmer un bandeau.

Sur les cinq clips négatifs, la persistance maximale d'un faux logo est de
16,7 % (Pretty Little Liars, 2/12) et celle d'un faux texte de 16,7 % ; aucun
ne passe 70 %. Le filtre élimine les faux positifs observés, mais un décor
statique ou un panneau fixe pourrait produire un cluster persistant : c'est
pourquoi la branche texte reste **annotation seulement**.

### 4.5 Latence, VRAM et résolution

Mesures sur la GTX 1650 4 Gio, batch 1, PyTorch fp32 + autocast fp16,
au seuil 0,25, 50 frames de la phase comparative :

| Taille (petit côté) | s/frame | 12 frames | VRAM pic | Rappel logo 30 frames | Rappel bande 10 A2 | Texte IoU médian |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 480 | 1,279 | 15,3 s | 1,47 Gio | 90,0 % | 90,0 % | 0,614 |
| 640 | 2,024 | 24,3 s | 1,84 Gio | 93,3 % | 90,0 % | 0,635 |
| 800 | 3,053 | 36,6 s | 2,31 Gio | 93,3 % | 100,0 % | 0,659 |

La ligne 480 reprend la moyenne des 592 inférences du run principal ; les
lignes 640/800 viennent de l'ablation à 50 inférences. L'écart 480
(1,279 vs 1,320 s) est une variance de mesure, pas un effet du corpus.

Le chargement du modèle depuis le cache HF prend 3,48 s. À 480 px, douze
frames coûtent 15,3 s à chaud, 18,8 s avec chargement ; six frames coûtent
7,7 s à chaud, 11,1 s avec chargement. À 800 px, douze frames coûtent 36,6 s,
soit plus que A1 (23,5 s) et A2 OCR (13,0 s) réunis.

**480 px est le seul réglage compatible avec le budget A2.** 640/800 restent
utiles pour une passe offline de découverte, où la latence n'est pas
critique et où le texte gagne en précision.

### 4.6 Cloud Replicate vs local

| Critère | Replicate `adirik/grounding-dino` | Local PyTorch 480 | Local ONNX |
| --- | --- | --- | --- |
| Coût | ~0,00098 $/run, ~0,012 $/12 frames | 0 $ marginal | 0 $ marginal |
| Latence | ~1 s/run sur L40S annoncé ; cold start et upload non mesurés | 1,28 s/frame, 15,3 s/12 frames | inconnue, export non prêt |
| VRAM | 0 côté client | 1,47 Gio | ~1,5 Gio attendu |
| Dépendance | réseau, API, jeton, quotas | Python, transformers, torch, CUDA | `ort` + ops custom |
| Données client | frames envoyées à un tiers | locales | locales |
| Pannes | réseau, 5xx, cold start, secret | OOM, driver, modèle absent | export/ops |
| Verdict | **hors production**, labo/QA avec accord client | **labo** | **cible long terme, pas prête** |

Replicate n'a pas été exécuté : `REPLICATE_API_TOKEN` est absent de
l'environnement. La page publique du modèle donne le prix, le matériel
(L40S) et « predictions typically complete within 1 second » ; notre client
`ReplicateBackend` est fourni mais **non validé**. Le wrapper attend des
requêtes séparées par des virgules et l'upload d'une image par requête :
douze frames = douze appels, plus la latence réseau et le cold start.

Le point bloquant n'est pas le coût (~0,83 $ pour 71 projets), mais la
**confidentialité des rushes client** et la dépendance réseau. Un recadrage
automatique ne doit pas dépendre d'un tiers pour lire des images de
programmes non diffusés.

### 4.7 Faisabilité ONNX

L'export ONNX officiel n'est pas disponible :

- la liste des architectures supportées par Optimum ONNX (2026) ne contient
  pas Grounding DINO ; l'issue `huggingface/optimum#1849` est ouverte depuis
  2024 ;
- le modèle utilise une attention déformable multi-échelle et un encodeur
  texte BERT ; un export custom doit remplacer/valider ces ops ;
- `transformers` 4.57 charge le modèle PyTorch, mais `optimum-cli export onnx`
  n'a pas de configuration `GroundingDinoOnnxConfig` ;
- l'ONNX Runtime du dépôt (`onnxruntime 1.23.0`) est CPU-only ; STEP2 utilise
  un autre runtime CUDA. Un DINO ONNX CUDA demanderait un build et une
  validation séparés.

**Conclusion** : à court terme, un micro-sidecar Python local est la seule
voie fiable ; à moyen terme, un export ONNX/TensorRT ou un moteur Rust natif
doit être évalué comme un projet distinct. Ce n'est pas un simple
`cargo add ort`.

## 5. Architecture proposée (non appliquée)

Cette section formalise les points d'attache si un pilote est un jour
autorisé. **Aucun de ces éléments n'est présent dans le code de production
aujourd'hui.**

### 5.1 Configuration TOML proposée

Sous `[steps.step8.post]`, toutes les clés seraient optionnelles avec
`deny_unknown_fields` et un défaut désactivé :

```toml
# Grounding DINO — laboratoire A2 (D-060). Désactivé par défaut.
a2_grounding_dino = false
a2_grounding_dino_backend = "local"          # "local" | "replicate"
a2_grounding_dino_model = "IDEA-Research/grounding-dino-tiny"
a2_grounding_dino_python = ""                # interpréteur du sidecar local
a2_grounding_dino_sidecar = "scripts/grounding-dino-sidecar.py"
a2_grounding_dino_size = 480                 # 480 = 1,5 Gio ; 800 = 2,3 Gio
a2_grounding_dino_frames = 12                # aligné sur A2_OCR_SAMPLES
a2_grounding_dino_box_threshold = 0.20
a2_grounding_dino_text_threshold = 0.20
a2_grounding_dino_min_persistence = 0.70
a2_grounding_dino_bottom_y_min = 0.60      # branche basse (logo/bandeau)
a2_grounding_dino_top_y_max = 0.25         # branche haute (logo)
a2_grounding_dino_timeout_s = 120
a2_grounding_dino_max_vram_mb = 2500
# Replicate : jamais de jeton ici.
a2_grounding_dino_replicate_model = "adirik/grounding-dino"
```

Le jeton Replicate reste `REPLICATE_API_TOKEN` et la version
`REPLICATE_GROUNDING_DINO_VERSION`, tous deux dans l'environnement. Aucune
valeur secrète dans le TOML.

### 5.2 Interface Rust proposée

Nouveau module `crates/workflow-steps/src/vision/grounding_dino.rs`, sans
dépendance à `qa` ni à `post` :

```rust
pub enum GroundingDinoBackend {
    Local { python: PathBuf, sidecar: PathBuf },
    Replicate { model: String },
}

pub struct GroundingDinoOptions {
    pub backend: GroundingDinoBackend,
    pub model: String,
    pub size: u32,
    pub frames: usize,
    pub box_threshold: f32,
    pub text_threshold: f32,
    pub min_persistence: f32,
    pub bottom_y_min: f32,
    pub top_y_max: f32,
    pub timeout: std::time::Duration,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetectedObject {
    pub label: String,
    pub score: f32,
    /// [x1, y1, x2, y2] en pixels source.
    pub bbox: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct PersistentObject {
    pub label: String,
    pub score: f32,
    pub bbox: [f32; 4],
    pub frames: usize,
    pub persistence: f32,
    pub center_std_px: f32,
}

pub trait OpenVocabularyDetector {
    fn detect(&mut self, frame: &std::path::Path, prompt: &str)
        -> anyhow::Result<Vec<DetectedObject>>;
}

pub fn cluster_persistent(
    detections: &[Vec<DetectedObject>],
    iou_min: f32,
    center_px: f32,
    min_ratio: f32,
) -> Vec<PersistentObject>;
```

Le module ne connaît ni `Action`, ni `Decision`, ni `CompMeta`. Il retourne
des objets ; `post/a2.rs` décide. Les arrondis JSON passent par
`workflow_schema::round::round_to`, jamais `f64::round`.

### 5.3 Contrat du sidecar local

Le prototype `serve` du script implémente déjà ce contrat JSON Lines :

```json
{"id":"54_ujud_03","image":"/tmp/frame.jpg","prompt":"logo . text . watermark . lower third . banner","box_threshold":0.20,"roi_y_min":0.60}
```

Réponse sur `stdout`, une ligne par requête :

```json
{"id":"54_ujud_03","detections":[{"label":"logo watermark","score":0.354,"bbox":[92.7,921.5,293.9,1028.2]}],"timing_ms":1210.4}
```

Les logs vont sur `stderr`. Le sidecar charge le modèle une fois, puis sert
N requêtes ; il doit être arrêté à la fin du lot pour libérer la VRAM avant
le prochain STEP2. Un timeout côté Rust tue le processus et ignore la clé
`grounding_dino`.

### 5.4 Règles d'intégration A2 (v1, revue seulement)

1. **Jamais d'action modifiée par Grounding DINO en v1.** Le bloc
   `details.grounding_dino` est additif ; `action`, `reason`, `confidence`
   et `scale_target` restent ceux de la géométrie + OCR.
2. **Logo inconnu** : si un cluster logo serré (largeur ≤ 0,35), persistant
   ≥ 70 %, n'a pas de correspondance dans `source_logos.toml`, ajouter
   `unknown_logo = true` et `logo_persistence`. La décision reste `review`
   ou `noop` ; le monteur voit « logo inconnu persistant » au lieu d'un
   simple « zone basse étroite ».
3. **Logo connu** : si le cluster logo serré recouvre le prior projeté
   (IoU ≥ 0,50), ajouter `source_logo_match = true`. Cela confirme le prior
   sans changer la décision.
4. **Bandeau** : si un cluster texte persistant ≥ 70 % couvre ≥ 35 % de la
   largeur dans la bande basse, ajouter `banner_confirmed = true`. En v1,
   cela **ne crée pas** d'`apply` ; cela enrichit la revue et peut confirmer
   un `apply` déjà décidé par l'OCR. Une future D-061 pourrait autoriser un
   `apply` sans OCR, mais seulement après un pilote sans faux `apply`.
5. **Confusion logo/bandeau** : si la seule boîte logo est large (> 0,35),
   ajouter `logo_merged_with_lower_third = true` et ne pas l'utiliser comme
   preuve de logo. C'est exactement le cas Enquête d'action/Enquête
   Exclusive observé.
6. **Échec** : sidecar absent, timeout, OOM, JSON invalide, modèle absent →
   warning, aucune clé `grounding_dino`, décision A2 strictement inchangée.
7. **Verrou matériel** : le sidecar local doit acquérir le verrou matériel
   (`/tmp/workflow-rs-hw.lock`) ou vérifier qu'aucun STEP2/3/4 ne tourne. Le
   serveur sérialise les runs, mais `post run` manuel peut coexister avec un
   STEP2 ; 1,47 Gio de VRAM sur 3,81 Gio utilisables ne pardonne pas.
8. **Ordre JSON** : `details.grounding_dino` est inséré après `ocr` et
   `source_logo`, avec `serde_json::Map` et `preserve_order`. Le schéma
   `post.json` reste 1 ; aucune clé de premier niveau n'est ajoutée.
9. **Provenance** : `backend`, `model`, `model_sha256` (local) ou `version`
   (Replicate), `size`, `frames`, `timing_ms`. Pas de jeton, pas d'URL
   signée, pas de chemin client dans les logs.

### 5.5 Tests à ajouter avant toute activation

- `cluster_persistent` : IoU 0,49/0,50, centre 10/11 px, 8/12 vs 9/12,
  6 vs 12 frames, deux clusters concurrents.
- Parsing du sidecar : réponse nominale, JSON invalide, timeout, code de
  sortie non nul, ligne vide, réponse sans `detections`.
- A2 avec un faux sidecar : décision et clés existantes inchangées ;
  `details.grounding_dino` absent en cas d'échec.
- A2 avec un sidecar synthétique : `unknown_logo`, `source_logo_match`,
  `banner_confirmed`, `logo_merged_with_lower_third` ; aucune action changée.
- Configuration : défaut `false`, `deny_unknown_fields`, valeur invalide
  (`size` < 320, `min_persistence` hors [0,1], timeout nul).
- Contrat : ordre des clés de `post.json` inchangé, `preserve_order` actif.
- Performance : mesurer 12 frames sur la GTX 1650 ; le test unitaire ne doit
  pas charger le modèle ni le GPU.

## 6. Décision D-060

### 6.1 Décision

**Grounding DINO n'entre pas dans A2 en production.** Il reste un outil de
laboratoire pour :

- proposer des logos inconnus au catalogue `source_logos.toml` ;
- qualifier les revues A2 (logo inconnu, bandeau persistant, confusion
  logo/bandeau) ;
- préparer un futur détecteur ouvert Rust/ONNX.

Le backend local PyTorch 480 px est retenu pour le laboratoire. Replicate est
écarté pour la production (données client, réseau, secret) et n'a pas été
exécuté. ONNX n'est pas prêt.

### 6.2 Motifs

1. **Rappel logos correct, précision insuffisante pour un gate** : 96,7 % à
   0,20 mais 25 % de faux positifs frame ; aucun seuil ne descend à 5 %.
2. **Confusion logo/bandeau** : le modèle produit souvent une boîte
   « logo + lower third » au lieu du logo seul ; c'est précisément le cas
   que A2 doit distinguer pour éviter un faux scale.
3. **Persistance prometteuse mais échantillon faible** : 0/5 clips négatifs
   persistants, mais IC 95 % [0 ; 43,4 %] ; il faut plus de clips, dont des
   décors statiques et des panneaux fixes.
4. **Texte sur deux styles seulement** : UJUD et 66 Minutes ; les bandeaux
   stylisés/dégradés ne sont pas couverts.
5. **Coût d'ingénierie** : sidecar Python, 1,47 Gio de VRAM, 15,3 s par
   projet à 12 frames, verrou matériel, tests de panne. Le gain actuel est
   une annotation, pas une décision.
6. **ONNX absent** : pas de chemin Rust natif immédiat ; un sidecar Python
   dans le pipeline contredit la règle « pas de fallback Python ».
7. **Cloud** : ~0,012 $/projet n'est pas le problème ; l'envoi de rushes
   client à Replicate et la dépendance réseau le sont.

### 6.3 Conditions de réouverture

Rouvrir D-060 (nouvelle décision) si :

- un export ONNX/TensorRT **ou** un moteur Rust natif (Candle/tch/ORT custom)
  reproduit les sorties avec un décodage de tokens validé ;
- le détecteur atteint, sur un corpus **indépendant** de au moins 20 clips
  négatifs et 20 logos inconnus : rappel ≥ 90 % à IoU 0,50, centre ≥ 95 %,
  ≤ 5 % de clips négatifs avec un cluster persistant, ≥ 70 % de persistance
  sur les vrais logos ;
- la latence reste ≤ 8 s pour 6 frames ou ≤ 16 s pour 12 frames sur la carte
  de production ;
- la branche logo serrée est stable et la confusion logo/bandeau est
  mesurée séparément ;
- un pilote `report_only` sur copies montre **0 faux `apply`** et un gain de
  tri des revues ;
- pour Replicate, le client a explicitement accepté l'envoi de frames et un
  DPA couvre la conservation.

### 6.4 Ce que le spike apporte malgré le refus

- Un **script reproductible** et un **manifeste de 242 frames** qui peuvent
  servir de banc d'essai à tout futur détecteur ouvert.
- Une **méthode de clustering** simple (IoU 0,5 + centre 10 px + 70 %) qui
  élimine les faux positifs observés sans toucher à A2.
- Une **branche logo serrée** (largeur ≤ 0,35) qui sépare le logo du bandeau,
  et une **branche texte** (largeur ≥ 0,20, y ≥ 0,60) qui confirme le
  bandeau.
- Un **contrat sidecar JSON Lines** et un prototype `serve` prêt à être
  branché sur un adaptateur Rust.
- Des **seuils mesurés** : 0,20 pour la découverte, 0,25 pour une annotation
  conservatrice, 480 px pour A2, 640/800 pour l'offline.

## 7. Reproduire

Le modèle et le venv sont hors dépôt (poids Apache-2.0, 659 Mio). Le script a
été exécuté avec le Python de `/mnt/venv_ext4/audio_env` (torch 2.8.0+cu128)
et les paquets Transformers installés dans `/tmp/opencode/gdino/site` :

```sh
# 1) Transformers + Hugging Face dans un répertoire temporaire
/mnt/venv_ext4/audio_env/bin/pip install --target /tmp/opencode/gdino/site \
  transformers==4.57.6 safetensors huggingface_hub

# 2) Manifeste (extrait les frames A2/négatives dans /tmp)
/mnt/venv_ext4/audio_env/bin/python scripts/spike-grounding-dino.py \
  build-dataset --frames-dir /tmp/opencode/gdino/frames

# 3) Évaluation locale 480 px
PYTHONPATH=/tmp/opencode/gdino/site HF_HOME=/tmp/opencode/gdino/hf \
/mnt/venv_ext4/audio_env/bin/python scripts/spike-grounding-dino.py run \
  --manifest docs/etudes/ETUDE-GROUNDING-DINO-A2/results/manifest.json \
  --output docs/etudes/ETUDE-GROUNDING-DINO-A2/results/results.json \
  --size 480 --phases prompts,validation

# 4) Rapport Markdown
/mnt/venv_ext4/audio_env/bin/python scripts/spike-grounding-dino.py report \
  --manifest docs/etudes/ETUDE-GROUNDING-DINO-A2/results/manifest.json \
  --results docs/etudes/ETUDE-GROUNDING-DINO-A2/results/results.json

# 5) Ablation 640/800 sur les 50 frames comparatives
for size in 640 800; do
  PYTHONPATH=/tmp/opencode/gdino/site HF_HOME=/tmp/opencode/gdino/hf \
  /mnt/venv_ext4/audio_env/bin/python scripts/spike-grounding-dino.py run \
    --manifest docs/etudes/ETUDE-GROUNDING-DINO-A2/results/manifest.json \
    --output docs/etudes/ETUDE-GROUNDING-DINO-A2/results/results-size${size}.json \
    --size $size --phases validation --only-split prompt
done

# 6) Prototype sidecar (une requête JSON par ligne)
echo '{"id":"t","image":"/tmp/opencode/gdino/frames/54_ujud_hotel_05.jpg","prompt":"logo . text . watermark . lower third . banner","box_threshold":0.2,"roi_y_min":0.6}' | \
  PYTHONPATH=/tmp/opencode/gdino/site HF_HOME=/tmp/opencode/gdino/hf \
  /mnt/venv_ext4/audio_env/bin/python scripts/spike-grounding-dino.py serve --size 480
```

Le script ne lit que le corpus en lecture seule et n'écrit que dans
`/tmp/opencode/gdino` et `docs/etudes/ETUDE-GROUNDING-DINO-A2/results/`.
Aucune écriture dans `/mnt/cache`, `runtime/` ou le dépôt Python.

## 8. Limites du spike

- **Corpus curé** : les 122 captures montrent le logo ; le rappel sur des
  frames quelconques n'est pas mesuré.
- **30 émissions non indépendantes** : variantes d'une même identité.
- **Cinq clips négatifs**, sans décor statique piégeur ; l'IC de 0/5 est
  large.
- **Deux styles de bandeau** (UJUD, 66 Minutes) ; pas de bandeau sombre,
  stylisé ou dégradé.
- **Un seul modèle** (tiny) ; base/Swin-B non testés.
- **Une seule carte** (GTX 1650 4 Gio) ; la production peut différer.
- **Replicate non exécuté** ; la comparaison cloud repose sur la
  documentation publique.
- **ONNX non exporté** ; la conclusion s'appuie sur l'absence de config
  Optimum et les ops custom, pas sur un essai d'export.
- **Pas de test de faux positif statique** ; un panneau fixe ou un décor
  répétitif pourrait passer la persistance.
- **Les métriques de texte** utilisent le bbox OCR A2 comme vérité terrain ;
  il peut être plus court que le texte visible.
- **Le prompt composite** a été choisi après observation ; un jeu de
  validation indépendant pourrait modifier les seuils.

## 9. Fichiers de preuve

| Fichier | Contenu |
| --- | --- |
| `results/manifest.json` | 242 frames, sources, splits, bbox GT, hash du catalogue |
| `results/results.json` | 592 inférences brutes (7 prompts + validation), timings, VRAM |
| `results/results.md` | Tableaux de métriques par prompt et par source |
| `results/results-size480.json` | Ablation 480 sur 50 frames |
| `results/results-size640.json` | Ablation 640 sur 50 frames |
| `results/results-size800.json` | Ablation 800 sur 50 frames |
| `results/results-logo-prompts.json` | Ablation de 5 prompts logo sur 50 frames |
| `results/results-logo-prompts.md` | Tableaux de l'ablation des prompts logo |
| `scripts/spike-grounding-dino.py` | Manifeste, inférence, métriques, sidecar |

Les frames extraites restent dans `/tmp/opencode/gdino/frames` et ne sont
pas versionnées. Les captures client restent dans
`/media/kidpixel/WORK/.../references_logos` et ne sont pas copiées dans le
dépôt.

## 10. Références

- D-051, D-052, D-055, D-056, D-058 dans
  `memory-bank/decisionLog.md`.
- `docs/lots/LOT-9-POST-STEP8.md` (A2, gate OCR, prior logo).
- `docs/lots/LOT-10-QA.md` (DINOv2, logos, OCR).
- `docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8.md` (seuils A2, corpus GOOD).
- `assets/models/qa/source_logos.toml` (30 émissions, 21 clean feeds,
  9 sans logo).
- Modèle : `IDEA-Research/grounding-dino-tiny`, Apache-2.0,
  <https://huggingface.co/IDEA-Research/grounding-dino-tiny>.
- Replicate : <https://replicate.com/adirik/grounding-dino> (L40S, ~1 s,
  ~0,00098 $/run, consulté le 2026-11-09).
- Optimum ONNX, architectures supportées :
  <https://huggingface.co/docs/optimum-onnx/onnx/overview> ; issue
  <https://github.com/huggingface/optimum/issues/1849>.

## 11. Suite (2026-11-09)

La décision D-060 est validée : Grounding DINO reste au laboratoire,
Replicate est définitivement écarté et Coral Edge TPU est hors sujet. La
suite est une **proposition B** de détecteur local léger exécuté nativement
en Rust via `ort` (YOLOX-Nano/NanoDet-Plus Apache-2.0), benchmarké sur le
même corpus de 242 frames : `docs/etudes/ETUDE-YOLO-ONNX-A2.md`.
