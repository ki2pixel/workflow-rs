# Décision D-057 — Détecteur de letterbox A1 natif opt-in

- **Statut** : acceptée
- **Date** : 2026-11-08
- **Portée** : A1 du post-STEP8, `crates/workflow-steps/src/post/{a1,detect}.rs`
- **Voir aussi** : `memory-bank/decisionLog.md` (D-057), D-056, D-058
  (mise en œuvre du mode gardé), `docs/etudes/ETUDE-VSLETTERBOX-A1.md`,
  `docs/lots/LOT-9-POST-STEP8.md`

## TL;DR

Le dépôt `Akatmks/vapoursynth-letterbox` n'est pas intégré comme dépendance :
c'est un filtre VapourSynth par frame, orienté nettoyage, sans agrégation
temporelle ni décision de crop. Son principe utile — EWMA 5σ sur la luminance
des lignes + validation de la frontière par contour — est retenu pour un
**spike natif Rust opt-in**. `cropdetect` reste le détecteur par défaut ; le
contrat A1 (`apply`/`review`/`noop`, `scale`, `Position Y`, `coverage`,
`confidence`) est inchangé.

## Contexte

A1 utilise `cropdetect=24:16:0` sur un échantillon de début de segment, puis
le 10e percentile des hauteurs de crop. L'étude du corpus GOOD (D-056) a
documenté trois limites :

1. faux vetos sur scènes sombres (304 seg 9 : faux haut de 642 px) ;
2. surestimation des barres par l'arrondi 16 px (305 : 86/82 px estimés
   contre 81/78 px réels) ;
3. sensibilité au bruit et aux transitions.

Le dépôt `Akatmks/vapoursynth-letterbox` propose un détecteur différentiel
(EWMA de Tony Finch, `alpha = 0,05`, seuil `μ + 5 × max(σ, 0,005)`) validé
par un masque de contours ExKirsch 5×5 (`dynamic_ref_thr = 2/3`). L'étude
demandée évalue sa transposition dans A1.

## Décision

1. **Ne pas intégrer** le plugin C++, le wrapper Python, `vs-jetpack`,
   `vsmasktools`, ni `clean_letterbox`.
2. **Retenir** pour un spike natif Rust : l'EWMA 5σ, le scan ligne à ligne,
   un Sobel Y 3×3 ciblé sur les lignes candidates, une couverture de colonnes
   et un clustering temporel des couples `(top, bottom)`.
3. **Ajouter** un mode opt-in `a1_detector = "native"` ; `cropdetect` reste
   le défaut. Aucun changement du chemin par défaut.
4. **Conserver** tous les vetos A1 : keyframes, asymétrie, `barred_frac`,
   `scale > 300 %`, `Position Y > 600`, couverture, différé A3.
5. **Ne pas utiliser** le p10 actuel pour agréger les mesures natives :
   cluster dominant ≥ 60 %, sinon `review`.
6. **Aucun code de production** n'est modifié par cette étude ; le spike est
   hors production, en lecture seule sur le corpus.

## Motifs

- **Gain mesuré** : sur le 305 seg 1, le prototype mesure 81/78 px (actif 921)
  contre 86/82 px (actif 912) pour `cropdetect=24:16:0` ; le scale exact passe
  de 210,5 % à 208,5 %, et avec la marge D-056 de 213,7 % à 211,6 % (humain
  210,0 %). Le gain est ~2,1 points de scale et 4–5 px par barre.
- **Faux positif supprimé** : sur le 304 seg 9, le faux haut de 642 px est
  rejeté par l'EWMA (gradient lent). Le masque Kirsch seul ne suffit pas :
  un bandeau large ou un ciel nuageux peut encore passer.
- **Limite structurelle** : le dépôt est par frame, sans persistance, sans
  symétrie, sans décision. Un p10 naïf sur ses mesures transformerait une
  frame à faux `top=935` en faux crop ; d'où le clustering temporel.
- **Coût** : `cropdetect` mesure 0,32–0,47 s par segment sur 304/305 ; le
  natif ciblé doit rester dans le même ordre de grandeur. Un ExKirsch 5×5
  plein cadre serait ~30× plus coûteux pour un gain marginal.

## Périmètre du spike

- Fixtures : 304 seg 9, 305 seg 1/9/17, 356 seg 2, 304 seg 34/36.
- Échantillon des 65 `apply` et 116 `review` segment de l'étude GOOD, dont
  les 8 applies non résolus et les 7 revues asymétriques résolues.
- 20 segments `noop` sans barre pour mesurer les faux applies.
- Métriques : erreur top/bottom ≤ 2 px, erreur de scale ≤ 1 point sur 305,
  zéro faux apply sur l'échantillon, temps ≤ 1,2 × `cropdetect`, variance
  frame-à-frame ≤ 1 px.
- Sortie : test `#[ignore]` ou exemple
  `crates/workflow-steps/examples/a1_letterbox_spike.rs`, écriture dans
  `/tmp/opencode/a1-letterbox-spike`, jamais dans `/mnt/cache` ou `runtime/`.

## Ce qui est conservé

- Le contrat `Decision::apply/review/noop`, les champs `scale_target`,
  `position_y_target`, `position_y_delta`, `coverage`, `confidence`.
- L'ordre d'insertion JSON (`preserve_order`) et les arrondis
  `workflow_schema::round::round_to`.
- Les vetos A1 et le différé A3 (`a1_defer_on_a3`).
- Le chemin `cropdetect` par défaut, byte-identique.
- Aucune dépendance Python/VapourSynth dans `workflow-cli`.

## Validation

- Rapport : `docs/etudes/ETUDE-VSLETTERBOX-A1.md`.
- Scripts et résultats : `docs/etudes/ETUDE-VSLETTERBOX-A1/`.
- Mesures : 304 seg 9, 305 seg 1/9/17, 304 seg 34, 356 seg 2 ; tests
  synthétiques texte/aplat/ciel/contraste.
- Limites : prototype Python approximatif, pas VapourSynth ; échantillon
  restreint ; images synthétiques non compressées ; `cropdetect=24:2:0` est
  une référence à 2 px, pas une vérité manuelle.

## Références

- `docs/etudes/ETUDE-VSLETTERBOX-A1.md`
- `docs/etudes/ETUDE-VSLETTERBOX-A1/README.md`
- `docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8.md` (D-056)
- `docs/lots/LOT-9-POST-STEP8.md`
- `crates/workflow-steps/src/post/{a1,detect}.rs`
- `crates/workflow-core/src/config.rs` (`PostConfig`)
- `memory-bank/decisionLog.md` (D-056, D-057)
