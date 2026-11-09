# Étude B — détecteur local ONNX pour logos inconnus et lower-thirds

- **Type** : proposition d'étude + spike
- **Statut** : Lot B0 **exécuté** (2026-11-09) ; B1/B2 à venir
- **Date** : 2026-11-09
- **Portée** : A2 du post-STEP8, `crates/workflow-steps/src/vision/`, `ort`
- **Décision liée** : D-060 (Grounding DINO laboratoire), D-052/D-056 (A2 conservateur)
- **Voir aussi** : `ETUDE-GROUNDING-DINO-A2.md`, `ETUDE-LOGOS-BANDEAUX-POST-QA.md`,
  `LOT-9-POST-STEP8.md`, `LOT-10-QA.md`

## 1. Résumé exécutif

D-060 a écarté Grounding DINO de la production : rappel correct sur les logos
connus, mais 25 % de faux positifs par frame, confusion logo/bandeau et
sidecar Python. La présente proposition explore l'**approche B** : un
détecteur compact exécuté **nativement en Rust via `ort`**, sans sidecar ni
dépendance Python d'exécution.

Objectif : détecter la **présence** d'un logo inconnu et d'un
lower-third/bandeau dans les zones utiles d'A2, avec une latence compatible
avec le post-STEP8, puis annoter la revue. Le détecteur ne doit jamais créer
un `apply`, retirer un veto ou modifier `scale_target`.

Trois constats structurent la proposition :

1. **YOLOv8n et YOLO11n répondent techniquement au besoin** (~5–6 Mo ONNX,
   environ 10–20 ms par frame sur la GTX 1650 selon l'export, à mesurer)
   mais sont sous **AGPL-3.0**. Sans licence Enterprise Ultralytics, ils ne
   conviennent pas à une intégration fermée.
2. **YOLOX-Nano** (Apache-2.0, 0,91 M paramètres, ~3,5 Mo ONNX) et
   **NanoDet-Plus-m** (Apache-2.0, 1,17 M paramètres, ~2,3 Mo FP16) sont les
   candidats production les plus crédibles. PicoDet-S (Apache-2.0) est une
   alternative si l'outillage Paddle est accepté.
3. **Un fine-tuning est nécessaire.** Les checkpoints COCO n'ont pas de
   classe `logo`/`lower_third` ; les détecteurs publics de logos visent des
   marques produit, pas des bugs de chaîne TV. Le fine-tuning peut être
   minimal (tête de détection) puis complet si nécessaire, à partir des
   références existantes et de compositions synthétiques.

La décision d'intégration sera prise après un benchmark sur les 242 frames
D-060, avec des seuils go/no-go explicites (§6.4). Tant que ces seuils ne
sont pas atteints, **aucun flag de production n'est ajouté**.

## 2. Besoin, périmètre et non-objectifs

### 2.1 Besoin

- Détecter un **logo de programme inconnu** du catalogue
  `source_logos.toml` et le signaler en `review` (« logo inconnu
  persistant »).
- Détecter un **lower-third / bandeau bas** même quand l'OCR échoue
  (police stylisée, fond sombre, incrustation dégradée).
- Fournir une **boîte spatiale** utilisable plus tard pour exclure le logo
  du groupe OCR dominant, sans créer de décision.
- Rester **local**, sans réseau, sans Replicate, sans Coral Edge TPU et sans
  Python à l'exécution.

### 2.2 Non-objectifs

- Identifier le logo (marque, chaîne, émission) : c'est le rôle du catalogue
  et de DINOv2, pas du détecteur.
- Remplacer la géométrie A2, l'OCR ou les garde-fous D-052/D-056.
- Créer un `apply`, retirer un `review` ou modifier `scale_target`.
- Utiliser un sidecar Python, un appel cloud ou un modèle TFLite/Edge TPU.
- Réintroduire Replicate ou Coral : les deux sont définitivement écartés.

### 2.3 Continuité avec l'étude logos/bandeaux

`ETUDE-LOGOS-BANDEAUX-POST-QA.md` rejetait YOLOv7/YOLOv8 pour la découverte
**open-set d'identités** : classes fermées, pas de temporalité, licences
copyleft. La présente proposition est différente :

- le détecteur est **class-agnostique** : il détecte la classe visuelle
  `logo`, pas une marque ;
- il est **entraîné sur notre corpus** de logos TV, pas sur COCO/Flickr ;
- il est **temporel** : persistance ≥ 70 % sur 12 frames, comme D-060 ;
- il utilise des modèles **Apache-2.0**, pas Ultralytics AGPL.

La reconnaissance d'identité reste hors périmètre : le détecteur dit
« logo inconnu présent », le catalogue et DINOv2 disent « lequel ».

## 3. Modèles candidats

### 3.1 Détecteurs génériques

Tailles ONNX mesurées au Lot B0 (batch 1, sans NMS). Les tailles des
modèles AGPL restent indicatives.

| Modèle | Licence | Params | Taille ONNX | Entrée | ONNX | Rôle |
| --- | --- | ---: | ---: | ---: | --- | --- |
| YOLOv8n | AGPL-3.0 | 3,2 M | ~6,4 Mo | 640 | oui | référence technique, **pas production** |
| YOLO11n | AGPL-3.0 | 2,6 M | ~5,4 Mo | 640 | oui | référence technique, **pas production** |
| **YOLOX-Nano** | **Apache-2.0** | **0,91 M** | **3,8 Mo FP32 / 2,0 Mo FP16** | 416 | oui | **candidat principal** |
| **NanoDet-Plus-m** | **Apache-2.0** | **1,17 M** | **5,8 Mo FP32 / 3,5 Mo FP16** | 320/416 | oui | **candidat secondaire** |
| PicoDet-S | Apache-2.0 | 1,18 M | quelques Mo | 320/416 | oui (Paddle2ONNX) | alternative |
| SSDLite320 MobileNetV3 | BSD-3-Clause | 3,4 M | ~13,4 Mo FP32 | 320 | oui (torchvision) | repli permissif |

Notes :

- YOLOv8n/YOLO11n sont les plus simples à entraîner et à exporter, mais leur
  licence AGPL-3.0 contamine l'application distribuée ou exposée en réseau.
  Sans licence Enterprise, ils restent des **bornes de comparaison**.
- YOLOX-Nano est le meilleur compromis licence/simplicité/taille. Son dépôt
  d'origine est ancien (2021–2022) ; utiliser un environnement isolé ou un
  fork maintenu Apache-2.0 pour l'entraînement, jamais pour l'inférence.
- NanoDet-Plus est très rapide, mais son export 416 pèse 5,8 Mo FP32
  (3,5 Mo FP16) et son décodage DFL est plus complexe (112 valeurs par
  ancre). Il reste le second candidat, mesuré au Lot B0.
- PicoDet-S demande PaddlePaddle/Paddle2ONNX ; il n'est retenu que si les
  deux premiers échouent.
- SSDLite320 est le repli si les toolchains YOLO anciennes posent problème :
  torchvision est maintenu, permissif, et l'export ONNX est direct. La
  qualité sur petits logos est probablement inférieure.

### 3.2 Checkpoints logos publics

| Checkpoint | Licence | Taille | Verdict |
| --- | --- | ---: | --- |
| `openfoodfacts/universal-logo-detector` | AGPL-3.0 | YOLOv8s, ~22 Mo | baseline labo uniquement ; licences et domaine inadaptés |
| `mettinski/logo-detector-rfdetr` | Apache-2.0 | RF-DETR-Small, 32,1 M, ~370 Mo | trop lourd, hors cible < 15 Mo |
| `ellabettison/Logo-Detection-finetune` | Apache-2.0 | YOLOS-small, 30,7 M, ~120 Mo | trop lourd, pas temps réel |
| `lakshya-rawat/YOLOV8s-Custom-Logo-detection` | tag Apache, base Ultralytics | YOLOv8s | licence ambiguë (AGPL en amont), écarté |

Aucun checkpoint public de **lower-third TV** n'a été identifié. Le
`News Graphic Dataset` (Roboflow, ~1,5 k images, classes DOG/lower third/
headline/ticker, <https://universe.roboflow.com/hello-on0wt/news-graphic-dataset-d3uc9>)
peut servir de pré-entraînement **si sa licence est compatible**, mais il ne
remplace pas un corpus français de divertissement. Le corpus A2 réel reste
la source principale.

### 3.3 Recommandation

Commencer par **YOLOX-Nano** en 416 et 640, avec **NanoDet-Plus-m** en
comparaison. Garder YOLO11n/YOLOv8n comme référence de qualité uniquement
si une licence Enterprise est disponible. Ne pas partir sur RF-DETR/YOLOS :
le budget < 15 Mo et < 15 ms/frame les exclut.

## 4. Fine-tuning : nécessaire, mais minimal au début

### 4.1 Réponse courte

**Un checkpoint existant ne suffit pas** pour les deux classes :

- COCO ne contient pas `logo` ni `lower_third` ; un détecteur COCO peut
  confondre un logo avec `tv`, `book` ou `clock`.
- Les détecteurs publics de logos sont entraînés sur des marques produit
  (FlickrLogos, Open Food Facts), pas sur des bugs de chaîne TV français.
- Aucun modèle public ne détecte les lower-thirds de divertissement.

Un **fine-tuning minimal** est donc requis. Il peut commencer par la seule
tête de détection, backbone gelé, puis être complété si le rappel ne passe
pas les seuils. C'est un coût de quelques heures à quelques jours, pas un
projet de plusieurs semaines.

### 4.2 Données disponibles

| Source | Volume | Usage |
| --- | ---: | --- |
| Dataset M6+ (`datasets_logos`) | 1 273 émissions + 226 chaînes, 1 499 PNG RGBA, 350 Mo | train complet + crops copy-paste |
| Références logos (`source_logos.toml`) | 30 émissions, 122 frames, variantes | test « vu » + crops pour compositions |
| Clean feeds | 21 émissions, 60 frames D-060 (extensible) | négatifs + fonds de composition |
| A2 lower-thirds | 5 clips × 12 frames = 60 | positifs réels + crops de bandeaux |
| `post.json` A2 | 5 clips | boîtes `details.bbox` et `details.ocr.bbox` |
| Frames négatives D-060 | 5 clips × 12 = 60 | test de faux positifs |

Le dataset M6+ est utilisé **à 100 %** pour l'entraînement, conformément à
la consigne de production : aucune émission M6+ n'est retirée du train. Les
122 frames D-060 servent de test « vu » (borne haute), pas de test
d'identité inconnue. L'open-set est mesuré sur un **jeu externe** de logos
absents du dataset : chaînes non-M6, chaînes internationales et autres
références. Les crops des identités M6+ peuvent alimenter les compositions ;
ceux du jeu externe sont interdits.

### 4.3 Compositions synthétiques

Le volume réel est trop faible pour un détecteur from scratch. La stratégie
retenue est le **copy-paste** :

- extraire les crops de logos des variantes `source_logos.toml` (RGBA) ;
- extraire les crops de lower-thirds des frames A2 et des `post.json` ;
- coller sur les clean feeds avec scale 0,4–2,0, position dans la zone A2,
  opacité 0,5–1,0, flou/compression/bruit, ombre et bord ;
- générer 2 000–5 000 images logo et 1 000–3 000 images lower-third ;
- conserver 20 % de fonds sans collage comme négatifs difficiles ;
- ajouter des négatifs piégeurs : logos statiques de décor, panneaux,
  crédits, scores, faux bandeaux.

Cette méthode est celle utilisée par les détecteurs de logos récents
(compositions copy-paste + quelques images réelles). Elle permet de couvrir
des identités et des fonds variés sans annoter des milliers de frames.

### 4.4 Splits et fuite de données

| Split | Règle |
| --- | --- |
| Train | **100 %** du dataset M6+ (1 273 émissions + 226 chaînes) + 3 clips A2 + clean feeds + compositions |
| Val | frames disjointes des clean feeds et des clips A2, sans retirer d'identité M6+ |
| Test connu (vu) | 122 frames D-060 + 60 frames A2 ; identités présentes au train, borne haute |
| Test inconnu (externe) | logos absents du dataset M6+ : chaînes non-M6, internationales, autres références |
| Négatifs test | 60 frames D-060 + clips supplémentaires si possible |

Aucune identité M6+ n'est tenue à l'écart : le test « connu » mesure la
qualité sur des frames réelles, pas la généralisation. La généralisation
open-set repose entièrement sur le jeu externe, qui ne doit jamais servir à
l'entraînement ni à la génération de compositions. Les splits val/test
portent sur des **frames et des clips**, pas sur des identités M6+.

### 4.5 Recette d'entraînement

- Entrée 416 et 640, batch 8–16 selon la VRAM (la GTX 1650 4 Gio suffit
  pour un nano modèle ; NanoDet annonce un batch 80 sur GTX 1060 6 Gio).
- 100–300 époques, AdamW ou SGD, cosine, warmup 5 époques.
- Augmentations : mosaic, scale, translate, brightness/contrast, blur,
  JPEG, opacité, copy-paste.
- Étape 1 : backbone gelé, tête seule, 20–30 époques. Si le rappel test
  plafonne, étape 2 : fine-tuning complet.
- Classes : `0 = logo`, `1 = lower_third`. `banner` n'est pas une classe
  séparée : un bandeau pleine largeur est un `lower_third` avec l'attribut
  géométrique `full_width` (largeur ≥ 0,50). Cela évite les boîtes
  concurrentes et la confusion de classes.
- Export : ONNX statique `1×3×H×W`, opset 12–17, **sans NMS** (NMS en Rust),
  FP32 puis FP16. INT8 seulement en diagnostic CPU/TensorRT, pas dans la
  cible CUDA EP (voir §7).

### 4.6 Critère d'arrêt du fine-tuning

Si après l'étape 1 le rappel logo **externe** est < 70 % ou si les faux
positifs frame restent > 10 %, ne pas lancer l'étape 2 « pour voir » :
réviser les données (négatifs, diversité) ou changer de famille de modèle.
Le fine-tuning ne doit pas masquer un problème de corpus. Sans jeu externe
exploitable, aucune conclusion open-set n'est autorisée.

## 5. Architecture Rust proposée

### 5.1 Module et types

Nouveau module `crates/workflow-steps/src/vision/logo_detector.rs` (ou
`yolo.rs`), sans dépendance à `post` :

```rust
pub struct DetectorConfig {
    pub model: PathBuf,
    pub device: ExecutionDevice,     // Cuda | Cpu (cpu = diagnostic)
    pub onnxruntime: Option<PathBuf>,
    pub input_size: u32,             // 416 ou 640
    pub conf_threshold: f32,         // 0.25
    pub iou_threshold: f32,          // 0.45
    pub class_names: Vec<String>,    // ["logo", "lower_third"]
    pub bottom_y_min: f32,           // 0.60
    pub top_y_max: f32,              // 0.25
}

pub struct Detection {
    pub class_id: usize,
    pub label: String,
    pub score: f32,
    pub bbox: [f32; 4],              // pixels source
}

pub struct LogoDetector { /* Session ORT, shape, metadata */ }

impl LogoDetector {
    pub fn load(config: &DetectorConfig) -> Result<Self>;
    pub fn detect(&mut self, frame: &BgrFrame) -> Result<Vec<Detection>>;
}
```

Le module ne connaît ni `Decision`, ni `A2Options`, ni `Meta`. Il expose des
boîtes ; `post` décide.

### 5.2 Session `ort`

Réutiliser `ort = 2.0.0-rc.10` (déjà dans `workflow-steps`), avec
`CUDAExecutionProvider` en production et `CPUExecutionProvider` uniquement
quand `device = "cpu"` est demandé explicitement. Le chargement suit le
modèle de `step2/engine.rs` :

- `ort::init_from` si `onnxruntime` est fourni, sinon `ort::init()` ;
- `GraphOptimizationLevel::Level3`, `with_parallel_execution(false)` ;
- CUDA device 0, `CuDNNConvAlgorithmSearch::Heuristic` (pas Exhaustive pour
  un petit modèle), TF32 désactivé par défaut ;
- `device = "cuda"` échoue explicitement si le provider CUDA n'est pas
  disponible : pas de repli CPU silencieux (le CPU reste un mode diagnostic
  demandé) ;
- warmup d'une frame factice avant la première mesure ;
- une seule session par run, partagée par les 12 frames.

Ne pas dupliquer la configuration CUDA de STEP2 : extraire un petit helper
`vision::inference::cuda_session` ou `vision::onnx` commun, testé par les
deux appelants. Le module QA reste CPU ; le détecteur A2 est le seul à
demander CUDA.

### 5.3 Prétraitement

- `BgrFrame` existant (ffmpeg `bgr24`) → RGB.
- Letterbox vers `input_size × input_size`, padding 114 (valeur usuelle
  YOLO) ou 0 selon le modèle ; conserver `scale` et `pad` pour remapper.
- Normalisation selon le modèle :
  - YOLOX : 0–255, pas de mean/std ;
  - YOLO : /255 ;
  - NanoDet : mean/std du config d'entraînement.
- Tenseur `f32 [1, 3, H, W]` NCHW via `Tensor::from_array`.
- Aucune allocation par frame si possible : réutiliser le buffer d'entrée.

### 5.4 Post-traitement

- Sortie YOLOX : `[1, N, 5 + C]` (cx, cy, w, h, obj, classes) ou
  `[1, C + 5, N]` selon l'export ; figer le format dans le manifeste.
- Décoder, filtrer `score ≥ conf`, NMS classe-aware avec `iou_threshold`.
- Remapper les boîtes dans les pixels source (inverse letterbox).
- Filtrer par ROI :
  - `logo` : bas-gauche (`x ≤ 0,40`, `y ≥ 0,55`) ou haut-gauche
    (`x ≤ 0,40`, `y ≤ 0,30`) ;
  - `lower_third` : bas de frame (`y ≥ 0,60`), largeur ≥ 0,20.
- Rejeter les boîtes > 60 % de la frame, largeur > 0,80 ou hauteur < 2 px,
  comme D-060.
- Si une boîte `logo` a une largeur > 0,35 et recouvre un `lower_third` :
  `logo_merged_with_lower_third = true`, ne pas la compter comme logo.

### 5.5 ROI et échantillonnage

Deux options à benchmarker :

1. **Pleine frame** 1920×1080 letterboxée en 640 : simple, mais les petits
   logos deviennent minuscules.
2. **ROI** : bande basse `y ∈ [0,55 ; 1]` pleine largeur + coin haut-gauche
   `x ∈ [0 ; 0,40]`, `y ∈ [0 ; 0,30]`, chaque ROI letterboxée en 416/640.
   Deux inférences par frame, mais une résolution effective bien meilleure.

Recommandation : commencer par la ROI basse seule (couvre logo bas-gauche +
lower-third), puis ajouter la ROI haute si les logos haut-gauche sont
fréquents. Réutiliser les **12 instants d'échantillonnage A2** (`A2_OCR_SAMPLES`)
pour éviter un décodage ffmpeg supplémentaire ; si l'OCR est désactivé,
échantillonner 12 frames également réparties.

### 5.6 Intégration A2 — annotation seule

Pour garantir qu'aucune décision ne change, **ne pas disperser l'appel dans
les nombreuses sorties de `decide_a2`**. `decide_a2` reste inchangé ; après
son retour, `post/run.rs` appelle un helper :

```rust
annotate_logo_detector(&mut decisions, &detector, &frames, &options)?;
```

Ce helper :

- ne touche qu'aux décisions `code == "A2"` ;
- ajoute `details.logo_detector` **après** `ocr` et `source_logo`, via
  `serde_json::Map` (`preserve_order`) ;
- ne modifie jamais `action`, `reason`, `confidence`, `scale_target` ;
- en cas d'échec (modèle absent, OOM, JSON invalide), log un warning et
  n'ajoute aucune clé.

Structure proposée :

```json
"logo_detector": {
  "backend": "onnx",
  "model": "yolox_nano_a2",
  "model_sha256": "...",
  "input_size": 640,
  "classes": {
    "logo": {
      "detected": true,
      "persistence": 0.9167,
      "bbox": [92.0, 925.0, 296.0, 1031.0],
      "score": 0.62
    },
    "lower_third": {
      "detected": true,
      "persistence": 1.0,
      "bbox": [100.0, 956.0, 1228.0, 1028.0],
      "score": 0.71
    }
  },
  "unknown_logo": true,
  "source_logo_match": false,
  "logo_merged_with_lower_third": false
}
```

Règles :

- `unknown_logo = true` si un cluster `logo` persistant ≥ 70 % n'a pas de
  correspondance dans `source_logos.toml` (IoU ≥ 0,50 ou centre dans la
  bbox du prior).
- `source_logo_match = true` si le cluster recouvre le prior.
- `lower_third` est purement informatif en v1 ; il ne crée pas d'`apply`.
- L'exclusion du texte OCR est **hors v1** : elle pourrait modifier le
  groupe dominant et donc une décision. Elle fera l'objet d'un pilote
  séparé, avec preuve « 0 faux `apply` ». En v1, le détecteur annote
  seulement.
- Si le détecteur est activé sans OCR, les clés `ocr` restent absentes ;
  `logo_detector` est indépendant.

### 5.7 Configuration proposée

Sous `[steps.step8.post]`, désactivé par défaut, `deny_unknown_fields` :

```toml
a2_logo_detector = false
a2_logo_detector_model = "qa/logo_yolox_nano_a2.onnx"
a2_logo_detector_onnxruntime = ""
a2_logo_detector_device = "cuda"          # cpu = diagnostic seulement
a2_logo_detector_size = 640               # 416 = plus rapide
a2_logo_detector_conf = 0.25
a2_logo_detector_iou = 0.45
a2_logo_detector_min_persistence = 0.70
a2_logo_detector_bottom_y_min = 0.60
a2_logo_detector_top_y_max = 0.25
```

`PostOptions` et `PostConfig` gagnent les mêmes champs ; `PostOptions`
résout `a2_logo_detector_model` via `paths.models`. Le manifeste
`assets/models/qa/manifest.toml` gagne une entrée `qa.logo_detector` avec
`size`, `sha256`, `license`, `source`, `classes`, `input`, `opset`. Le
binaire ONNX reste gitignoré.

### 5.8 Verrou matériel et VRAM

Le détecteur utilise le GPU : il doit acquérir
`HardwareLock::acquire(lock_path, false)` avant de charger la session et le
conserver pendant l'inférence. `post run` manuel peut coexister avec un
STEP2/3/4 ; le verrou est la seule protection fiable. En cas de lock occupé,
le détecteur est désactivé avec un warning, A2 reste inchangé.

Budget : viser ≤ 1,5 Gio de VRAM pour la session (nano modèle + CUDA
context). La GTX 1650 a 4 Gio, dont ~3,81 Gio utilisables ; A2 OCR est CPU.
Si la VRAM est insuffisante, désactiver le détecteur, ne pas réduire les
garde-fous.

### 5.9 Tests

- `letterbox` : dimensions, scale, padding, remap inverse.
- Décodage : format de sortie, scores, classes, boîtes, NMS, seuils.
- ROI : bas-gauche, haut-gauche, bas de frame, hors ROI.
- Persistance : IoU 0,49/0,50, centre 10/11 px, 8/12 vs 9/12, 6 vs 12.
- Configuration : défaut `false`, `deny_unknown_fields`, size invalide,
  seuils hors [0,1], device inconnu.
- A2 : avec un détecteur synthétique, `action`/`reason`/`confidence`/
  `scale_target` inchangés ; `logo_detector` ajouté après `source_logo`.
- Échec : modèle absent, OOM, timeout, sortie vide → aucune clé, décision
  inchangée.
- JSON : ordre des clés, `preserve_order`, écriture temp+rename.
- Performance : test unitaire sans modèle ni GPU ; benchmark séparé.

## 6. Protocole de benchmark sous `ort`

### 6.1 Phase 0 — export et vérification

1. Entraîner/convertir le modèle dans un environnement labo isolé
   (PyTorch/Paddle), jamais dans le runtime Rust.
2. Exporter ONNX statique `1×3×416×416` et `1×3×640×640`, opset 12–17,
   sans NMS ; FP32 puis FP16.
3. Vérifier en Python : sortie PyTorch vs ONNX Runtime, écart max < 1e-4
   (FP32) et < 1e-2 (FP16), même NMS.
4. Calculer `sha256`, taille, licence, opset ; inscrire au manifeste.
5. Vérifier que `ort` charge le modèle avec CUDA EP et CPU EP.

### 6.2 Phase 1 — microbenchmark Rust

Binaire labo `crates/workflow-steps/examples/yolo_bench.rs` (ou
`workflow-cli spike-yolo` hors production) :

- charge le modèle via `ort`, CUDA EP, warmup 20 itérations ;
- décode les PNG/JPEG via une dev-dependency `image` (ou ffmpeg) : cette
  dépendance ne doit pas entrer dans le binaire de production ;
- 100 itérations par frame, batch 1 ;
- mesure séparément : décodage image, prétraitement, `session.run`,
  post-traitement/NMS ;
- p50, p95, moyenne, min/max ; VRAM pic via `nvidia-smi` ou statistiques
  du provider ;
- charge CPU/GPU, taille du modèle, temps de chargement ;
- sortie JSONL par frame + résumé JSON.

Le décodage image ne doit pas être compté dans la latence modèle : charger
les images en mémoire avant la boucle, ou mesurer les deux.

### 6.3 Phase 2 — métriques de détection

Sur les frames de test D-060 (partition §4.4), mêmes définitions que
D-060 :

| Métrique | Définition |
| --- | --- |
| Rappel logo | IoU ≥ 0,50 avec une bbox `gt_logo`, seuil balayé |
| Centre GT | centre de la détection dans une bbox GT |
| IoU médian | médiane des meilleurs IoU |
| Rappel lower-third | IoU ≥ 0,50 avec `gt_band`, ou centre dans la bande |
| Texte | IoU ≥ 0,30 avec `gt_text` (informatif) |
| FP frame | frame négative avec au moins un `logo` en ROI ou un `lower_third` en bande |
| FP clip | cluster persistant ≥ 70 % sur un clip négatif |
| Confusion | `logo_merged_with_lower_third` / frames logo |
| Persistance | 12 frames, IoU ≥ 0,50 + centre ≤ 10 px |
| Latence | p50/p95 par frame, GPU, batch 1 |
| VRAM | pic de la session |
| Taille | fichier ONNX |

Balayer les seuils `0,15 / 0,20 / 0,25 / 0,30 / 0,40 / 0,50` et les NMS
`0,40 / 0,45 / 0,50`. Reporter les IC 95 % (Wilson) pour les taux.

### 6.4 Seuils go/no-go

Pour passer en pilote A2 `report_only` :

- **Rappel logo connu (vu)** ≥ 95 % à IoU 0,50 sur les 122 frames D-060
  et les 60 frames A2 ; borne haute, car les identités sont au train.
- **Rappel logo inconnu** ≥ 80 % sur le jeu externe (IC 95 % reporté ;
  l'échantillon reste limité). Le test « vu » ne prouve pas l'open-set.
- **Rappel lower-third** ≥ 90 % sur les 60 frames A2.
- **FP frame** ≤ 5 % sur les 60 négatifs D-060 (D-060 était à 25 %).
- **FP clip** : 0/5 clips négatifs persistants, puis 0/20 sur le corpus
  élargi.
- **Confusion logo/bandeau** ≤ 5 % des frames logo.
- **Latence** p95 ≤ 15 ms/frame GPU à 640, ou ≤ 10 ms à 416, prétraitement
  et NMS inclus.
- **Taille** ≤ 15 Mo, **VRAM** ≤ 1,5 Gio.
- **A2** : 0 différence d'`action` sur 20 projets en `report_only`, aucune
  nouvelle clé hors `details`, aucun faux `apply`.

Si un seul seuil critique échoue, le détecteur reste labo. Ne pas baisser
les seuils après avoir vu les résultats.

### 6.5 Comparaison avec D-060

Le rapport final doit contenir un tableau unique :

| Modèle | Licence | Taille | Entrée | Rappel logo | Rappel bande | FP frame | FP clip | Latence p95 | VRAM | Verdict |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| A2 actuel (géométrie+OCR) | — | — | — | — | — | — | — | — | — | référence |
| Grounding DINO tiny (D-060) | Apache-2.0 | 659 Mo | 480 | 96,7 % | 98,3 % | 25 % | 0/5 | 1 279 ms | 1,47 Gio | labo |
| YOLOX-Nano COCO zéro-shot | Apache-2.0 | 3,5 Mo | 416/640 | à mesurer | à mesurer | à mesurer | à mesurer | à mesurer | à mesurer | baseline |
| YOLOX-Nano fine-tuné | Apache-2.0 | 3,5 Mo | 416/640 | cible ≥ 95 % | cible ≥ 90 % | cible ≤ 5 % | cible 0/5 | cible ≤ 15 ms | ≤ 1,5 Gio | candidat |
| NanoDet-Plus fine-tuné | Apache-2.0 | 2,3 Mo | 320/416 | cible ≥ 95 % | cible ≥ 90 % | cible ≤ 5 % | cible 0/5 | cible ≤ 15 ms | ≤ 1,5 Gio | candidat |
| YOLO11n fine-tuné | AGPL-3.0 | 5,4 Mo | 640 | référence | référence | référence | référence | référence | référence | licence bloquante |

Les 96,7 % de D-060 portent sur les 122 frames du catalogue. Le candidat
est évalué sur ces 122 frames (identités vues au train) et sur le jeu
externe (identités inconnues) ; les deux chiffres doivent figurer, car le
premier est optimiste.

### 6.6 Commandes prévues

```sh
# Labo uniquement : entraînement et export (environnement isolé)
python scripts/train-logo-detector.py --config docs/etudes/yolo-a2/train.toml
python scripts/export-logo-detector.py --checkpoint runs/best.pt --format onnx

# Benchmark Rust natif, sans sidecar
cargo run --release -p workflow-steps --example yolo_bench -- \
  --model /tmp/yolo-a2/logo_yolox_nano.onnx \
  --manifest docs/etudes/ETUDE-GROUNDING-DINO-A2/results/manifest.json \
  --device cuda --size 640 --output /tmp/yolo-a2/bench.jsonl

# Rapport comparatif (offline, ne touche pas la production)
python scripts/report-logo-detector.py \
  --bench /tmp/yolo-a2/bench.jsonl \
  --baseline docs/etudes/ETUDE-YOLO-ONNX-A2/results/baseline.json
```

Les scripts d'entraînement/export sont **labo uniquement** ; aucun n'est
appelé par le serveur, le CLI de production ou A2.

## 7. Risques et parades

| Risque | Impact | Parade |
| --- | --- | --- |
| Licence AGPL Ultralytics | contamination juridique | YOLOX/NanoDet/PicoDet Apache-2.0 ; YOLO11n seulement avec licence Enterprise |
| Domaine TV français absent des datasets publics | rappel faible | copy-paste sur clean feeds + corpus réel + test tenu à l'écart |
| Petits logos à 1080p | rappel faible | ROI 416/640, pas de full-frame 640 seul |
| Confusion logo/lower-third | faux `review`/faux `apply` | NMS classe-aware, branche logo ≤ 0,35, `logo_merged_with_lower_third`, annotation seule |
| Lower-third animé | persistance instable | 12 frames, seuil 70 %, revue documentée |
| INT8 non accéléré sur CUDA EP | latence dégradée | FP16/FP32 d'abord ; INT8 CPU/TensorRT seulement en diagnostic |
| Anciennes toolchains YOLOX/NanoDet | entraînement difficile | environnement isolé, fork maintenu, repli SSDLite320 |
| VRAM partagée avec STEP2/3/4 | OOM | `HardwareLock`, `gpu_mem_limit`, désactivation si lock occupé |
| Dérive du modèle | résultats non reproductibles | manifeste SHA-256, version, licence, jeu de test gelé |
| Faux `apply` | régression production | annotation seule, `report_only`, diff JSON, 0 faux `apply` |

## 8. Plan de travail proposé

### Lot B0 — faisabilité (exécuté le 2026-11-09)

- vérifier les licences et les tailles des candidats ;
- exporter YOLOX-Nano et NanoDet-Plus en ONNX ;
- écrire le microbenchmark Rust `ort` ;
- mesurer zéro-shot et la latence sur 20 frames ;
- décision : B1 recommandé, YOLOX-Nano 416 FP32 en candidat principal.

### Lot B1 — logos (5 jours)

- construire le dataset copy-paste sur 100 % du dataset M6+ et le jeu
  externe open-set ;
- fine-tuner YOLOX-Nano 416 FP32 (principal) et 640 FP16 (secondaire) ;
- évaluer sur D-060 (identités vues) et sur le jeu externe (inconnues) ;
- produire le tableau comparatif D-060 vs YOLO.

### Lot B2 — lower-thirds et pilote (5 jours)

- annoter/extraire ≥ 20 clips de lower-thirds (ou synthèse équivalente) ;
- fine-tuner la classe `lower_third` ;
- mesurer confusion, persistance et faux positifs ;
- pilote A2 `report_only` sur 20 copies, diff JSON ;
- rédiger la décision D-061 (go/no-go) si les seuils sont atteints.

Total indicatif : **~2 semaines**, hors annotation manuelle lourde.

## 9. Livrables

- `docs/etudes/ETUDE-YOLO-ONNX-A2.md` (ce document) ;
- `docs/etudes/ETUDE-YOLO-ONNX-A2/results/` : manifeste, métriques,
  tableau comparatif, pas de médias client ;
- `scripts/train-logo-detector.py`, `scripts/export-logo-detector.py`,
  `scripts/report-logo-detector.py` (labo uniquement) ;
- `crates/workflow-steps/examples/yolo_bench.rs` (benchmark Rust, hors
  production) ;
- entrée `qa.logo_detector` dans `assets/models/qa/manifest.toml` ;
- si go : module `vision/logo_detector.rs`, configuration, tests, docs
  `LOT-9`, ADR D-061.

## 10. Lot B0 — résultats d'exécution (2026-11-09)

Statut : **exécuté**. Aucun fichier de production, aucune configuration et
aucun appel A2 n'ont été modifiés. Le binaire de benchmark est
`crates/workflow-steps/examples/yolo_bench.rs` ; les mesures sont dans
`results/`.

### 10.1 Environnement et protocole

| Élément | Valeur |
| --- | --- |
| GPU | GTX 1650 4 Gio, driver 535.309.01 |
| CUDA / cuDNN | CUDA 12.x, cuDNN 9 (paquets `nvidia-*` de l'env transnet) |
| ONNX Runtime | 1.23.2 CUDA, `GraphOptimizationLevel::Level3` |
| `ort` | 2.0.0-rc.10, CUDA EP, `CuDNNConvAlgorithmSearch::Heuristic`, TF32 off |
| Python labo | 3.10, PyTorch 2.8.0+cu128, ONNX 1.19.1 |
| Corpus | 20 frames D-060 : 8 références, 6 A2, 6 négatifs |
| Mesure | 20 warmup, 100 itérations/frame, batch 1, p50/p95 |

Les 20 frames sont un sous-échantillon régulier du manifeste D-060. Les
mesures incluent le redimensionnement bilinéaire, la conversion CHW, la
session `ort` et le décodage/NMS ; elles excluent le décodage ffmpeg et les
E/S disque. Le détail par frame est dans les JSON de `results/`.

### 10.2 Exports

| Modèle | Entrée | Sortie | Opset | FP32 | FP16 |
| --- | --- | --- | ---: | ---: | ---: |
| YOLOX-Nano | 1×3×416×416 | 1×3549×85 | 12 | 3,8 Mo | 2,0 Mo |
| YOLOX-Nano | 1×3×640×640 | 1×8400×85 | 12 | 3,9 Mo | 2,1 Mo |
| NanoDet-Plus-m | 1×3×416×416 | 1×3598×112 | 11 | 5,8 Mo | 3,5 Mo |

- YOLOX : dépôt `Megvii-BaseDetection/YOLOX` tag `0.3.0`, poids
  `yolox_nano.pth` (release `0.1.1rc0`), Apache-2.0. Export statique sans
  NMS, boîtes décodées (`cx, cy, w, h`) incluses dans la sortie. Le script
  d'export officiel a été adapté à PyTorch 2.8 (`torch.onnx._export` →
  `torch.onnx.export(..., dynamo=False)`) ; le modèle n'est pas modifié.
- NanoDet : dépôt `RangiLyu/nanodet`, configuration
  `nanodet-plus-m_416.yml`, poids `nanodet-plus-m_416_checkpoint.ckpt`
  (release `v1.0.0-alpha-1`), Apache-2.0. Export statique sans NMS ; le
  décodage DFL (112 valeurs par ancre) est fait en Rust.
- FP16 :
  `onnxruntime.transformers.float16.convert_float_to_float16(..., keep_io_types=True)`,
  entrées/sorties FP32 conservées.

Empreintes complètes :

```text
yolox_nano_416.onnx        c8179298c5571561865963310142691b7a38b800e01fce1377efd51d927b14d8
yolox_nano_416_fp16.onnx   b7158abb068a21aec489dd115db51fe18428703168c7f063ab9b216680e58b6d
yolox_nano_640.onnx        386cc5ef465330ed0f12707b07523d7c3dafec7702e92c2c467fddcc50f56fa5
yolox_nano_640_fp16.onnx   f5d51402823db71f605d8d697917de246ec5b6b70216d1f74d11145cc7ebb7c9
nanodet_plus_m_416.onnx    897d8d5691b8598524035868647e58f53681b5d0c51258655be84b77a15f8b75
nanodet_plus_m_416_fp16.onnx 8bd5f5d42024a61fa5db39ef7b0ae75219461cfb3512aa9814e0b55f93714bef
```

Parité PyTorch → ONNX Runtime CPU, même frame, mêmes prétraitements :

| Comparaison | Écart absolu max | Écart moyen |
| --- | ---: | ---: |
| YOLOX 416 | 1,19e-2 px | 5,54e-6 px |
| YOLOX 640 | 1,12e-2 px | 6,95e-6 px |
| NanoDet 416 | 3,62e-5 | 6,65e-7 |

Parité FP16 vs FP32, détections après seuil 0,25 et NMS 0,45, sur les 20
frames :

| Modèle | Détections FP32 | Détections FP16 | Appariées IoU ≥ 0,5 | Non appariées |
| --- | ---: | ---: | ---: | ---: |
| YOLOX 416 | 86 | 86 | 86 | 0 |
| YOLOX 640 | 94 | 93 | 93 | 1 (score 0,254) |
| NanoDet 416 | 181 | 127 | 118 | 63 |

YOLOX FP16 est stable au seuil de décision. NanoDet FP16 ne l'est pas :
63 détections FP32 disparaissent ou changent, dont une à 0,663. NanoDet
reste donc en **FP32** tant que la conversion FP16 n'a pas été corrigée.

### 10.3 Latence et VRAM

Mesures Rust, GTX 1650, 20 frames, 100 itérations, p50 / p95 en ms. « run »
est la session `ort` seule ; « total » ajoute prétraitement et post-traitement.

| Modèle | Entrée | Précision | Prétrait. | `run` | Post | Total | Δ VRAM |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| YOLOX-Nano | 416 | FP32 | 3,03 / 3,18 | 3,90 / 5,29 | 0,04 / 0,05 | **7,01 / 8,40** | +107 Mo |
| YOLOX-Nano | 640 | FP32 | 6,84 / 7,10 | 9,46 / 9,78 | 0,08 / 0,10 | 16,36 / 16,80 | +161 Mo |
| NanoDet-Plus-m | 416 | FP32 | 5,16 / 5,45 | 5,28 / 6,48 | 0,43 / 0,47 | 10,82 / 12,16 | +109 Mo |
| YOLOX-Nano | 416 | FP16 | 2,91 / 3,10 | 3,50 / 5,18 | 0,04 / 0,05 | 6,52 / 8,19 | +112 Mo |
| YOLOX-Nano | 640 | FP16 | 6,81 / 7,08 | 6,77 / 8,09 | 0,08 / 0,10 | **13,69 / 15,00** | +107 Mo |
| NanoDet-Plus-m | 416 | FP16 | 5,18 / 5,35 | 4,46 / 6,04 | 0,43 / 0,47 | 10,11 / 11,64 | +134 Mo |

Le delta VRAM est la différence entre le premier échantillon `nvidia-smi`
et le pic ; il inclut le contexte CUDA et fluctue avec les autres processus
(107–161 Mo observés). Les pics absolus sont 304–349 Mo en FP32 et
305–344 Mo en FP16, mais les bases diffèrent (188–212 Mo) : seul le delta
est comparable.

Diagnostic CPU, même code, 10 frames, 20 itérations, sans CUDA :

| Modèle | Prétrait. | `run` | Post | Total |
| --- | ---: | ---: | ---: | ---: |
| YOLOX 416 | 4,40 / 4,52 | 27,45 / 35,28 | 0,02 / 0,03 | 31,96 / 40,30 |
| NanoDet 416 | 8,04 / 10,91 | 20,48 / 40,48 | 0,48 / 0,59 | 29,10 / 52,47 |

Le chemin CPU n'est pas optimisé (pas de parallel execution, threads par
défaut) : il sert uniquement de diagnostic de chargement.

### 10.4 Sanity check zéro-shot

Les poids COCO n'ont ni classe `logo` ni classe `lower_third` : ils ne
peuvent pas mesurer le rappel ni les faux positifs de la cible. Sur les 20
frames, YOLOX 416 déclenche au moins un objet COCO sur 7/8 références,
6/6 bandeaux et 6/6 négatifs ; la classe dominante est `person` (61 des 86
détections). YOLOX 640 et NanoDet donnent le même ordre de grandeur. Ce
n'est **pas** un taux de faux positifs logo : c'est la démonstration que le
zéro-shot COCO est inutilisable pour A2 et que le fine-tuning B1 est
obligatoire.

### 10.5 Go/no-go B0

| Critère | Cible | Résultat | Verdict |
| --- | --- | --- | --- |
| Taille | ≤ 15 Mo | 2,0–5,8 Mo | OK |
| `run` p95 à 416 | ≤ 15 ms | 5,29 (YOLOX) / 6,48 (NanoDet) | OK |
| Total p95 à 416 | ≤ 15 ms | 8,40 / 12,16 | OK |
| Total p95 à 640 | ≤ 15 ms | 16,80 FP32 / 15,00 FP16 | partiel |
| VRAM | ≤ 1,5 Gio | +107 à +161 Mo | OK |
| Export et parité | charge en CUDA EP | oui | OK |
| Rappel logo / bande | ≥ 95 % / ≥ 90 % | non évalué | B1 |
| Open-set externe | ≥ 80 % | non évalué | B1 |
| A2 sans différence | 0 diff | non exécuté | B2 |

Conclusion B0 : **réussi**. Le candidat principal pour B1 est
**YOLOX-Nano 416 FP32** (total p95 8,40 ms, parité FP16 parfaite mais sans
gain de latence à 416). **YOLOX-Nano 640 FP16** est le second candidat si
les petits logos manquent de rappel ; son total p95 est à la limite de
15 ms. **NanoDet-Plus-m 416 FP32** reste une alternative valide
(12,16 ms), mais
son décodage DFL est plus coûteux et sa conversion FP16 est rejetée.

### 10.6 Limites

- 20 frames seulement, une seule GPU, pas de mesure sur le corpus complet
  de 242 frames. Les chiffres de latence sont stables, pas les chiffres de
  qualité.
- Le prétraitement Rust est un bilinéaire scalaire. À 640, il pèse 6,8–7,1 ms
  et fait dépasser 15 ms au total en FP32 ; le modèle seul reste sous
  10 ms. Une optimisation FFmpeg/ROI est possible en B2.
- La pré-vérification Python des modèles FP16 a donné des latences plus
  élevées que le benchmark Rust ; seules les mesures Rust avec
  `GraphOptimizationLevel::Level3` sont retenues. Les réglages de session
  changent fortement le résultat FP16.
- INT8 n'a pas été testé : la GTX 1650 (TU117) n'a pas de tensor cores et
  le chemin INT8 CUDA EP/TensorRT est un spike séparé. L'étude §7 le
  classe déjà en diagnostic.
- Aucun poids dérivé du dataset client ne doit être commité : les ONNX
  restent locaux et gitignorés, seuls les hashes et métadonnées sont
  versionnés.

### 10.7 Suite B1

1. Construire le pipeline copy-paste avec le canal alpha des 1 499 PNG
   (1 273 émissions + 226 chaînes), sur les clean feeds et les frames A2.
2. Entraîner YOLOX-Nano 416 en priorité, 640 en second ; 100 % du dataset
   M6+ au train, jeu externe pour l'open-set.
3. Évaluer sur les 122 frames D-060 (vues) et sur le jeu externe
   (inconnues), avec les seuils du §6.4 ; ne pas conclure sur l'open-set
   sans jeu externe.
4. Si les seuils passent, pilote A2 `report_only` sur 20 copies et diff
   JSON ; aucune modification de `a2.rs` avant ce pilote.

## 11. Références

- D-060 : `docs/decisions/DECISION-GROUNDING-DINO-A2.md` et
  `docs/etudes/ETUDE-GROUNDING-DINO-A2.md`.
- D-052/D-056 : `memory-bank/decisionLog.md`, `docs/lots/LOT-9-POST-STEP8.md`.
- Étude logos/bandeaux : `docs/etudes/ETUDE-LOGOS-BANDEAUX-POST-QA.md`.
- YOLOX (Apache-2.0) : <https://github.com/Megvii-BaseDetection/YOLOX>.
- NanoDet-Plus (Apache-2.0) : <https://github.com/RangiLyu/nanodet>.
- PicoDet (Apache-2.0) : <https://github.com/PaddlePaddle/PaddleDetection/tree/release/2.8/configs/picodet>.
- YOLO11 (AGPL-3.0) : <https://huggingface.co/Ultralytics/YOLO11>.
- YOLOv8 (AGPL-3.0) : <https://github.com/ultralytics/ultralytics>.
- Open Food Facts universal-logo-detector (AGPL-3.0) :
  <https://huggingface.co/openfoodfacts/universal-logo-detector>.
- RF-DETR logo detector (Apache-2.0, trop lourd) :
  <https://huggingface.co/mettinski/logo-detector-rfdetr>.
- ONNX Runtime CUDA EP :
  <https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html>.
- ONNX Runtime quantification :
  <https://onnxruntime.ai/docs/performance/model-optimizations/quantization.html>.
- torchvision SSDLite320 :
  <https://pytorch.org/vision/stable/models/generated/torchvision.models.detection.ssdlite320_mobilenet_v3_large.html>.
