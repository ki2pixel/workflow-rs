# Spécification Rust — corrections post-STEP8 issues du Corpus_Exemple_4

> **Statut** : spécification implémentée derrière des flags désactivés par
> défaut, avec tests unitaires et validation locale. Le shadow run full corpus a
> été exécuté en lecture seule ; ses résultats corrigés sont intégrés ci-dessous.
> Les flags restent à valider avant activation en production (4 faux applies A1
> résiduels, 4 récupérations A7 plausibles).
> **Source** : `benchmark_corpus4/out/RAPPORT-CORPUS4-POST-STEP8.md`,
> 1 200 projets mesurables, 40 572 segments appariés.
> **Cible** : `workflow-rs`, `crates/workflow-steps/src/post/` et
> `crates/workflow-core/src/config.rs`.

## 1. Principes

1. **Review-first** : aucun nouveau `apply` n'est créé par ces corrections ;
   elles ne font que rétrograder des applies douteux ou récupérer des missions
   manquées.
2. **Pas de garde-fou géométrique aveugle** : la hauteur active seule ne
   distingue pas une scène sombre d'un vrai letterbox. Le cas 361
   (`TEVA_Incroyable embouteillage`) a 28 applies valides à `active_height=960`,
   identiques en géométrie aux 32 faux applies de 363.
3. **Natif comme second avis** : le détecteur `native_letterbox` existant est
   utilisé uniquement sur la bande ambiguë, pas sur tous les segments.
4. **Traçabilité** : chaque décision modifiée porte un motif et un détail JSON
   (`a1_ambiguous_native`, `a7_global_fallback`, etc.) pour l'audit.

---

## 2. P0-A1 — confirmation native sur la bande ambiguë

### 2.1 Constat

| Projet | Applies | `active_height` | Cible | Faux applies |
|---|---:|---:|---:|---:|
| `363 Camille / OCLC_DION_AFIDA_DION` | 33 (34 actions) | 960–976 | 199,7–203 % | **32** |
| `361 Camille / TEVA_Incroyable embouteillage…` (2 projets) | 58 | 960 | 203 % | **0** |

Les deux projets ont des barres symétriques de 104–120 px, `barred_frac=0` et
`crop.ratio=1,0` dans la majorité des segments. Un seuil `900 ≤ active_height
< 1040` rétrograderait donc **61 applies valides** (3 à 199,7 % + 58 à 203 %).
La différence est visuelle (noir du letterbox vs scène sombre), pas
géométrique : elle relève du détecteur natif.

### 2.2 Modification proposée

Fichier : `crates/workflow-steps/src/post/a1.rs`.

Ajouter une option explicite :

```rust
pub struct A1Options {
    // ...
    /// Second avis natif sur la bande 950–990 (normalisée 1080p).
    /// Désactivé par défaut ; ne crée jamais d'apply.
    pub native_ambiguous_guard: bool,
}
```

Ajouter le prédicat :

```rust
const AMBIGUOUS_ACTIVE_MIN_1080: f64 = 950.0;
const AMBIGUOUS_ACTIVE_MAX_1080: f64 = 990.0;
const AMBIGUOUS_BARRED_MAX: f64 = 0.20;

fn ambiguous_letterbox(details: &Value, source_height: u32) -> bool {
    let Some(active) = details.get("active_height").and_then(Value::as_u64) else {
        return false;
    };
    let active_1080 = active as f64 * 1080.0 / source_height.max(1) as f64;
    let barred = details
        .get("barred_frac")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    (AMBIGUOUS_ACTIVE_MIN_1080..=AMBIGUOUS_ACTIVE_MAX_1080).contains(&active_1080)
        && barred < AMBIGUOUS_BARRED_MAX
}
```

Dans `decide_a1`, **après** le raffinage natif existant et avant de pousser la
décision :

```rust
let (mut decision, mut delta) = decide_segment(/* ... */);

if options.refine_native && decision.action == Action::Apply {
    match refine_native(/* ... */) {
        Ok(Some((refined, refined_delta))) => {
            decision = refined;
            delta = refined_delta;
        }
        Ok(None) => {}
        Err(error) => { /* review, comme aujourd'hui */ }
    }
}

if options.native_ambiguous_guard
    && decision.action == Action::Apply
    && ambiguous_letterbox(&decision.details, source_height)
{
    // Si refine_native est actif, le natif a déjà eu sa chance ci-dessus.
    // S'il n'a pas confirmé, on ne peut pas distinguer scène sombre et
    // letterbox : review.
    let mut details = decision.details.clone();
    details["a1_ambiguous_native"] = json!(true);
    details["a1_ambiguous_reason"] = json!("native_not_confirmed");
    decision = Decision::review(
        "A1",
        decision.target.clone(),
        "bande 950–990 px ambiguë (scène sombre ou letterbox), natif non confirmé",
        0.35,
        details,
    );
    delta = None;
}
```

**Important** : `refine_native` actuel renvoie `None` aussi bien quand le natif
ne trouve rien que quand le raffinement échoue aux vetos. Pour le garde-fou, il
faut distinguer « natif indisponible » de « natif rejette » :

```rust
enum NativeVerdict {
    Confirmed(Decision, Option<f64>),
    Rejected,
    Unavailable,
}
```

`Rejected` et `Unavailable` -> `review`. `Confirmed` -> conserver l'apply.

### 2.3 Portée

- Le garde-fou ne s'applique qu'aux `apply` A1, jamais aux `review`/`noop`.
- Il est **désactivé par défaut** ; il nécessite `refine_native` ou un appel
  direct à `native_letterbox`.
- Les segments 66 Minutes (`active_height` 688, 864, 1056) ne sont pas dans la
  bande. Ils relèvent d'un **prior d'émission** appliqué dans `run.rs` :
  ```rust
  if program == "66 Minutes" && decision.action == Action::Apply {
      decision = Decision::review(
          "A1", decision.target.clone(),
          "insert letterboxé 66 Minutes : revue systématique",
          0.35, decision.details.clone(),
      );
      delta = None;
  }
  ```
  Le shadow montre 3/3 faux applies 66 Minutes évités, 0 apply valide perdu.

### 2.4 Tests

| Test | Attendu |
|---|---|
| `ambiguous_letterbox` avec `active=976`, `barred=0`, source 1080 | `true` |
| `ambiguous_letterbox` avec `active=1024`, `barred=0` | `false` |
| `ambiguous_letterbox` avec `active=976`, `barred=0.4` | `false` |
| Projet 363, natif activé | 32 faux -> review ; 1 apply valide (Segment 30) rétrogradé |
| Projet 361 (2 projets), natif activé | 58 applies valides conservés |
| Projet 101 (`66 Minutes`), prior activé | 3 applies -> review |

### 2.5 Résultats du shadow run

| Indicateur | Avant | Après shadow |
|---|---:|---:|
| Applies A1 | 5 939 | 5 901 |
| Faux applies | **39** | **4** |
| Applies valides | 5 900 | 5 897 |
| Revues A1 ajoutées | — | 38 |

- 93 segments ambigus, **58 confirmés** (tous les applies de 361),
  **35 rejetés** (32 faux de 363 + 3 applies valides).
- 3 faux applies 66 Minutes traités par le prior d'émission.
- 4 faux applies résiduels : 25 (EQA), 34 (BA DIM PA), 83 (revue de presse),
  280 (Les Marseillais).
- 3 applies valides rétrogradés : 2 punch-ins sémantiques de `Piquantes`
  (échelles humaines 210–214 %) et le segment 30 de 363 (237 %).
- Le shadow utilise un **proxy luminance/gradient**, pas le détecteur Rust
  exact : les 93 segments doivent être rejoués avec `native_letterbox` avant
  activation.

Le critère « 361 ≥ 55/58 » est atteint (58/58) ; le critère « 363 ≥ 30/32 »
aussi (32/32).

---

## 3. P0-A7 — fallback global par titre et cutoff temporel

### 3.1 Constat

- 38 revues `video_not_in_mission` ont un titre retrouvé ailleurs, dont 23 avec
  logos et 15 sans logo. Après contrôle des dates, seules **3** missions sont
  dans une fenêtre de 30 jours ; les 35 autres viennent de titres réutilisés
  entre janvier et juin 2026.
- Le lot est déduit du dossier (`ClientMission::lot_from_folder`), or le dossier
  `X Camille` n'est pas le lot e-mail (offset −209 à −136).
- `run_a7` ne charge que les missions des lots déduits des dossiers : un
  fallback global ne trouverait rien si le lot est faux.

### 3.2 Modification implémentée

Fichiers : `crates/workflow-core/src/config.rs`,
`crates/workflow-steps/src/post/a7.rs`, `crates/workflow-steps/src/post/run.rs`,
`crates/workflow-cli/src/main.rs`.

1. **Charger les missions récentes** : quand `a7_global_fallback` est actif,
   `load_missions_for_folders` ajoute toutes les missions du cache (limite
   2 000) à côté des missions des lots du run. Le cache est déjà borné par la
   purge de rétention.
2. **Fenêtre temporelle** : une mission n'est acceptée que si sa date est dans
   `[génération du projet − 30 j, génération du projet + 1 j]`. La borne basse
   est indispensable : sans elle, 35 des 38 revues et 312 des 313 candidats
   correspondaient à des missions de janvier à juin 2026.
3. **Normalisation** : le fallback compare `filename_key`, `stem_key` et une
   clé dédiée qui retire le suffixe `_TAT_<chaîne>` (`COUPLE LIBRE_TAT_Téva`
   -> `couple libre`). Une variante par tokens (Jaccard ≥ 0,85, au moins
   5 tokens) couvre les titres dont l'ordre des mots change.
4. **Résolution A7** :
   - garder d'abord la mission du lot et `match_video(stem)` ;
   - si `NotFound` ou aucune mission du lot, chercher dans l'index global ;
   - si exactement une instruction candidate -> l'utiliser avec
     `match_source: "global_fallback"` ;
   - si plusieurs candidates avec des instructions identiques -> utiliser la
     plus récente ; si les instructions divergent -> `ambiguous_global_video` ;
   - si la candidate a `no_logo` -> `noop no_logo` ;
   - si `logo_unresolved` -> `review logo_unresolved`.
5. **Traçabilité** : le bloc `mission` contient `match_source`
   (`lot` ou `global_fallback`).

### 3.3 Tests

| Test | Attendu |
|---|---|
| `COUPLE LIBRE` (mission `COUPLE LIBRE_TAT_Téva`, lot 209, 3 j) | noop `no_logo` via `global_fallback` |
| `Maeva…` (mission `Greg…`, lot 277, −1 j) | apply `les marseillais.png` (tokens réordonnés) |
| `BA DIM PA` (mission de janvier, 299 j) | hors fenêtre, revue inchangée |
| Deux missions même titre, instructions différentes | review `ambiguous_global_video` |
| Mission postérieure au cutoff haut | ignorée |
| Projets sans mission | décision inchangée |

### 3.4 Résultats du shadow run (fenêtre 30 j)

| Indicateur | Valeur |
|---|---:|
| Revues `video_not_in_mission` récupérées | **3 / 38** |
| dont applies | **2** |
| dont noops `no_logo` | **1** |
| Revues hors fenêtre, conservées en revue | **35** |
| Nouveaux candidats (projets sans A7) | **1** |
| dont applies | **1** |
| Candidats hors fenêtre, ignorés | **312** |
| Projets sans mission | 719 |

Les 3 récupérations plausibles sont `COUPLE LIBRE` (noop, 3 j), `Maeva…`
(apply, −1 j) et `Booder…` (apply, 1 j). Le candidat supplémentaire est
`LMF - Tout se joue…` (apply, 9 j). Les 347 autres candidatures reposent sur
des missions de plus de 30 jours : elles restent en revue.

**Critères atteints** : 3 récupérations plausibles, 0 mission > 30 j appliquée,
0 apply sur les 719 projets sans mission.

---

## 4. P1-A2 — triage des revues et signaux forts

### 4.1 Constat

| Type de revue | Revues | Action humaine | Lecture |
|---|---:|---:|---|
| `décor bas` | 305 | 14,1 % | bruit majoritaire |
| `logo étroit` | 201 | 14,9 % | bruit majoritaire |
| `bande haute` | 150 | 24,0 % | mixte |
| `bande animée` | 118 | 37,3 % | review |
| `bande partielle` | 52 | 17,3 % | bruit |
| `sous-titres` | 20 | 65,0 % | signal fort |
| `bande sombre` | 17 | 94,1 % | signal très fort |
| `autre` | 56 | 78,6 % | à qualifier |

### 4.2 Modification proposée

Fichier : `crates/workflow-steps/src/post/a2.rs`.

1. **`décor bas`** : passer en `noop` uniquement si la zone est haute
   (`height > 200`), sans OCR et si la cible géométrique est **hors de
   190–300 %**. Le shadow montre qu'un noop dès que la cible sort de 190–230 %
   supprime 61 actions humaines ; la plage 190–300 % est le compromis retenu
   (131 noops, 14 actions perdues).
2. **`logo étroit`** : **ne pas nooper** sur le seul critère `width < 0,35`.
   Le shadow mesure 30 actions humaines perdues sur 201 revues. Seul un OCR
   explicitement `logo_only` peut autoriser un `noop`.
3. **Signaux forts** : `bande sombre` avec OCR confirmé
   (`text_persistence ≥ 0,6`, `dominant_ratio ≥ 0,8`, `score ≥ 0,55`),
   géométrie stable (`center_y_std ≤ 12`, `height_std ≤ 12`) et
   `scale ≤ 300 %` -> `apply`. Le shadow produit 13 applies, dont 12 confirmés
   par `a2_missed` et 12 par une action humaine.
4. **`sous-titres`** : rester en `review` ; le texte change souvent et le
   shadow ne montre pas de gain sûr.
5. **Priors par émission** : ne pas coder les noms d'émissions dans `a2.rs`.
   Ajouter une section `post.a2.emission_priors` dans la config, chargée par
   `run.rs`, avec :
   ```json
   {
     "Un Jour, un Doc": {"review_banner_apply": true},
     "L'Île de la tentation": {"review_banner_apply": true},
     "M6 - Romance": {"review_banner_apply": true},
     "PP - Postdiff/Prédiff": {"review_banner_apply": true},
     "Les Marseillais": {"review_only": true},
     "Piquantes !": {"review_only": true},
     "Cauchemar en cuisine": {"review_only": true}
   }
   ```
   Le programme est déjà disponible dans le manifeste/`PostOptions` ; à défaut,
   le prior ne s'applique pas.
6. **`bande animée`** : reste `review`, aucun apply automatique.

### 4.3 Tests

- `décor bas` sans OCR, hauteur 250 px, cible 213 % -> `review`.
- `décor bas` sans OCR, hauteur 250 px, cible 180 % -> `noop`.
- `logo étroit` avec OCR `logo_only` -> `noop` ; sans OCR -> `review`.
- `bande sombre` OCR confirmé, géométrie stable -> `apply`.
- `sous-titres` -> `review`.
- Les 235 bandeaux manqués doivent rester `review` ou `apply`, jamais `noop`.

### 4.4 Critères d'acceptation

- Réduction des revues A2 mesurée par le shadow : **14,2 %** (131 noops sur
  924), pas 30–50 %.
- Actions humaines perdues : **≤ 15** (shadow : 14).
- Applies `bande sombre` confirmés : **≥ 12 / 13**.
- Aucun `noop` sur `logo étroit` sans OCR `logo_only`.

---

## 5. P1-A3 / A4 / A5 — lots suivants

### 5.1 A3 — pilote keyframes

- Fichier : `a3.rs`.
- Activer un `apply` conservateur uniquement pour les candidats
  `strong_candidate` / `high_confidence`.
- Garde-fous : segment ≥ 1 s, amplitude ≥ 150 px, visage dans le cadre,
  2–3 clés maximum.
- Shadow sur 100 projets, priorité Gulli, M6 magazines, 6ter.
- Critère : ≥ 80 % des revues actées conservées, 0 régression de cadrage.

### 5.2 A4 — réconciliation des cuts

- Fichier : `a4.rs` + `run.rs`.
- Ajouter une revue projet si `|n_good - n_raw| >= 2`, avec les compteurs et les
  segments ajoutés/supprimés.
- Ne pas modifier les bornes : A4 reste `review only` pour ce lot.
- Priorité : Les Apprentis Aventuriers (+5,57), Les Marseillais (+3,95),
  L'Amour est dans le pré (+2,92), Vous les femmes (+2,67).

### 5.3 A5 — apply mono-segment

- Fichier : `a5.rs`.
- Passer en `apply` les cas mono-segment, sans keyframes,
  `resolution=resolved`, gaps symétriques.
- Conserver `review` pour les cas multi-segments ou visage hors cadre.
- Shadow sur 50 projets ; critère : 100 % des 143 revues du corpus résolues,
  sans sur-zoom.

---

## 6. Déploiement

| Flag | Défaut | Portée | Validation |
|---|---|---|---|
| `a1_native_ambiguous_guard` | `false` | A1, bande 950–990 | shadow 93 segments (363 + 361 + autres) |
| `a1_66m_prior` | `false` | A1, programme 66 Minutes | shadow 3/3 faux évités |
| `a2_review_triage` | `false` | A2, `décor bas` hors 190–300 % | shadow 924 revues (14,2 % de noops) |
| `a2_emission_priors` | `false` | A2, config | échantillon 100 projets |
| `a7_global_fallback` | `false` | A7, fenêtre 30 j | shadow 3/38 + 1 candidat |
| `a3_apply_pilot` | `false` | A3 | 100 projets |
| `a4_cut_reconciliation` | `false` | A4 | 100 projets |
| `a5_apply_mono` | `false` | A5 | 50 projets |

**Ordre conseillé** : A7 (gain immédiat, faible risque), A1 (natif ambigu +
prior 66 Minutes), A2 (triage conservateur), puis A3/A4/A5. Chaque flag doit
être réversible sans migration de données.

### 6.1 Shadow run A1 — exécuté

- 1 203 projets, 5 939 applies, 93 segments ambigus, 38 revues ajoutées.
- 363 : **32/32** faux applies rétrogradés ; 361 : **58/58** applies valides
  conservés ; 66 Minutes : **3/3** faux évités.
- 3 applies valides perdus (2 punch-ins Piquantes, 1 segment 363) ;
  faux applies totaux : **39 -> 4**.
- Le contrôle natif est un proxy luminance/gradient, validé sur 363 et 361 ;
  le détecteur Rust exact doit être rejoué sur les 93 segments avant activation.

### 6.2 Shadow run A7 — exécuté (fenêtre 30 j)

- 38 revues `false_negative_candidate` : **3** récupérées dans la fenêtre
  (2 applies + 1 noop `no_logo`) ; **35** hors fenêtre restent en revue.
- 313 projets sans A7 avec un titre retrouvé : **1** candidat dans la fenêtre
  (apply) ; **312** hors fenêtre ignorés.
- Aucun apply sur les 719 projets sans mission. Sans borne basse, 35 revues et
  312 candidats auraient appliqué des logos de missions de janvier à juin 2026.

---

## 7. Fichiers et points d'insertion

| Fichier | Rôle |
|---|---|
| `crates/workflow-steps/src/post/a1.rs` | `A1Options`, `decide_a1`, prédicat `ambiguous_letterbox` |
| `crates/workflow-steps/src/post/detect.rs` | `native_letterbox`, `NativeLetterbox` (réutilisé) |
| `crates/workflow-steps/src/post/a2.rs` | branches `décor bas`, `logo étroit`, `bande sombre` |
| `crates/workflow-steps/src/post/a7.rs` | `apply`, fallback global, `mission_block` |
| `crates/workflow-steps/src/post/run.rs` | `run_a7`, cutoff, chargement des missions |
| `crates/workflow-core/src/mission.rs` | `match_video`, index par titre, `lot_from_folder` |
| `crates/workflow-cli/src/main.rs` | `load_missions_for_folders`, `load_recent_missions` |
| `crates/workflow-cli/src/config.rs` | flags et priors d'émission |

---

*Spécification dérivée des sorties `benchmark_corpus4/out/`. Les chemins de
corpus sont en lecture seule ; les tests de shadow doivent travailler sur des
copies ou des liens, jamais sur le corpus d'origine.*
