# Décision D-060 — Grounding DINO hors production A2

- **Statut** : acceptée (2026-11-09), **laboratoire seulement**
- **Date** : 2026-11-09
- **Portée** : A2 du post-STEP8, `source_logos.toml`, QA des logos
- **Voir aussi** : D-051, D-052, D-055, D-056, D-058, D-059 (réservé A1),
  `docs/etudes/ETUDE-GROUNDING-DINO-A2.md`,
  `docs/lots/LOT-9-POST-STEP8.md`, `docs/lots/LOT-10-QA.md`

## TL;DR

Le spike Grounding DINO tiny (Swin-T, 659 Mio) sur 242 frames est
**prometteur pour la découverte hors ligne**, pas pour un gate A2 :

- 96,7 % des 122 logos du catalogue à IoU ≥ 0,50 au seuil 0,20, IoU médian
  0,90 sur ces 122 frames ;
- 98,3 % des 60 bandeaux A2 au seuil 0,20, mais deux styles seulement ;
- 25 % de faux positifs **par frame** sur 60 frames négatives, et 0/5 clips
  après persistance ≥ 70 % (IC 95 % [0 ; 43,4 %]) ;
- 5/5 logos persistants ≥ 70 %, 3/5 textes persistants ;
- 1,279 s/frame à 480 px, 1,47 Gio de VRAM, +15,3 s pour 12 frames ;
- Replicate non exécuté (pas de jeton), ONNX non prêt (pas de config
  Optimum).

**Décision** : aucun flag de production, aucune modification de
`PostConfig`, `a2.rs` ou `config/config.toml`. Grounding DINO reste un outil
de laboratoire pour proposer des logos inconnus au catalogue et qualifier
les revues A2. Le backend local PyTorch 480 px est retenu pour le
laboratoire ; Replicate est écarté pour la production ; ONNX/TensorRT ou un
moteur Rust natif sont les conditions de réouverture.

## Contexte

A2 détecte les bandeaux bas persistants et les logos de programme. Le
registre `source_logos.toml` couvre 30 émissions ; un logo inconnu échappe au
prior spatial. L'OCR PP-OCRv6 confirme le texte des bandeaux, mais échoue sur
les polices stylisées, les bandeaux sombres et les incrustations dégradées.
DINOv2 confirme des logos texte haute bande, mais reste un probe fermé.

Grounding DINO est un détecteur ouvert guidé par texte. Le spike devait
répondre à trois questions :

1. détecte-t-il les logos absents du catalogue et les bandeaux bas ?
2. la persistance temporelle élimine-t-elle les faux positifs ?
3. cloud Replicate ou local ONNX/PyTorch/CUDA ?

## Décision

1. **Pas d'intégration A2 en production.** Aucun flag
   `a2_grounding_dino`, aucune clé TOML, aucun appel de modèle dans
   `post/a2.rs`.
2. **Usage laboratoire** : découverte de logos inconnus, annotation des
   revues, mesure de persistance, préparation d'un futur détecteur ouvert.
3. **Backend local PyTorch 480 px** pour le laboratoire. Le sidecar
   `scripts/spike-grounding-dino.py serve` implémente le contrat JSON Lines.
4. **Replicate écarté pour la production** : données client, dépendance
   réseau, secret, cold start. Il reste un outil de prototypage si un jeton
   est fourni et si le client accepte l'envoi de frames.
5. **ONNX non prêt** : pas de config d'export Optimum, ops de deformable
   attention, encodeur BERT. Un micro-sidecar local est la seule voie
   court terme.
6. **Règle de sûreté** : si Grounding DINO est un jour activé, il ne peut
   ni créer un `apply`, ni retirer un veto, ni modifier `scale_target` ; il
   annote `details.grounding_dino` et laisse A2 décider.

## Motifs

- **Précision insuffisante pour un gate** : 25 % de faux positifs frame au
  seuil 0,20 ; aucun seuil ne descend à 5 %.
- **Confusion logo/bandeau** : le modèle produit souvent une boîte
  « logo + lower third » (Enquête d'action, Enquête Exclusive, Zone
  Interdite, Les Traîtres) au lieu du logo seul. C'est exactement ce que A2
  doit distinguer pour éviter un faux scale.
- **Persistance prometteuse mais échantillon faible** : 0/5 clips négatifs
  persistants, mais IC 95 % [0 ; 43,4 %] ; il faut des décors statiques et
  des panneaux fixes.
- **Texte sur deux styles** : UJUD et 66 Minutes seulement.
- **Coût d'ingénierie** : sidecar Python, 1,47 Gio de VRAM, 15,3 s par
  projet à 12 frames, verrou matériel, tests de panne.
- **ONNX absent** : pas de chemin Rust natif immédiat ; un sidecar Python
  contredit la règle « pas de fallback Python ».
- **Cloud** : ~0,012 $/projet n'est pas le frein ; l'egress de rushes client
  et la dépendance réseau le sont.

## Conséquences

- `source_logos.toml` reste la source de vérité pour A2 ; les logos inconnus
  restent en `review`.
- Le spike fournit un banc d'essai réutilisable (242 frames, manifeste,
  métriques) pour tout futur détecteur ouvert.
- La branche logo serrée (largeur ≤ 0,35) et la branche texte
  (largeur ≥ 0,20, y ≥ 0,60) sont documentées comme point de départ.
- Le clustering IoU ≥ 0,50 + centre ≤ 10 px + persistance ≥ 70 % est le
  filtre de référence.
- Aucun impact sur la suite de tests, les performances ou les contrats JSON.

## Alternatives

| Option | Verdict |
| --- | --- |
| A. Intégrer maintenant en `review` opt-in | Rejetée : sidecar Python, 15,3 s/projet, pas de validation indépendante |
| B. Intégrer maintenant comme gate d'`apply` | Rejetée : faux positifs frame, confusion logo/bandeau, pas d'ONNX |
| C. Laboratoire seulement (retenue) | Sure, réversible, prépare la découverte de logos |
| D. Replicate en production | Rejetée : données client, réseau, secret, cold start |

## Conditions de réouverture

Rouvrir D-060 (nouvelle décision) si :

- un export ONNX/TensorRT **ou** un moteur Rust natif reproduit les sorties
  avec un décodage de tokens validé ;
- sur un corpus indépendant (≥ 20 clips négatifs, ≥ 20 logos inconnus) :
  rappel ≥ 90 % à IoU 0,50, centre ≥ 95 %, ≤ 5 % de clips négatifs avec un
  cluster persistant, ≥ 70 % de persistance sur les vrais logos ;
- latence ≤ 8 s pour 6 frames ou ≤ 16 s pour 12 frames sur la carte de
  production ;
- la confusion logo/bandeau est mesurée séparément et la branche logo
  serrée reste stable ;
- un pilote `report_only` sur copies montre **0 faux `apply`** ;
- pour Replicate, accord client explicite et DPA sur la conservation.

## Validation

- 242 frames, 592 inférences, 7 prompts + validation, 3 résolutions.
- Aucun code Rust n'a été modifié. `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` et
  `cargo test --workspace` (557 tests) sont verts.
- Aucune écriture dans `/mnt/cache`, `runtime/` ou le dépôt Python.
- Preuves : `docs/etudes/ETUDE-GROUNDING-DINO-A2/results/`.

## Références

- `docs/etudes/ETUDE-GROUNDING-DINO-A2.md`
- `scripts/spike-grounding-dino.py`
- `assets/models/qa/source_logos.toml`
- `crates/workflow-steps/src/post/a2.rs`
- `crates/workflow-steps/src/vision/source_logo.rs`
- `crates/workflow-core/src/config.rs` (`PostConfig`)
- Modèle : `IDEA-Research/grounding-dino-tiny` (Apache-2.0)
- Replicate : `adirik/grounding-dino`
- Optimum ONNX : architecture non supportée, issue #1849
