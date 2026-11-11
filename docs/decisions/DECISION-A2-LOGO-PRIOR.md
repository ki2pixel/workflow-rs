# Décision D-062 — activation pilote de `a2_logo_prior`

- **Statut** : acceptée (2026-11-11), **pilote contrôlé**
- **Date** : 2026-11-11
- **Portée** : A2 du post-STEP8, `config/config.toml`
- **Voir aussi** : D-052, D-055, D-060, D-061,
  `docs/etudes/ETUDE-CORPUS-248-POST-STEP8.md`,
  `docs/lots/LOT-11-A1-VETO-LATERAL.md`

## TL;DR

Sur 248 projets finalisés (`Corpus_Exemple_3`), la shadow evaluation
`a2_logo_prior=true` + `a2_logo_detector=true` crée **un seul `apply`**, et il
est correct : `257 Camille — 66M VENTES` passe de `review` à `apply` 218,2 %,
contre 215,3 % pour le monteur (+2,9 points, dans la tolérance ±8). Aucun
`apply` perdu, aucun faux apply. Le prior ajoute deux fausses revues
(`108`, `243`) et 15 reformulations de motif, sans coût CPU mesurable.

**Décision** : `a2_logo_prior = true` en pilote contrôlé.
`a2_logo_detector` reste `false` : aucune décision modifiée, 2 313 ms/projet
en moyenne, et 41/199 confirmations toutes étroites qui ne couvrent aucune
des 38 actions de bandeau.

## Contexte

D-052 a livré le catalogue `qa/source_logos.toml` et le flag
`a2_logo_prior`, désactivé depuis D-055. D-060 a écarté Grounding DINO ;
B2.1 a livré YOLOX-Nano 416 en annotation seule, `a2_logo_detector = false`.
L'étude 248 chiffre la post-STEP8 : A2 est sûr (9 `apply`, 287 segments,
0 revert, 0 sur-zoom) mais rate 13 recadrages de bandeau sur 175 revues
mesurables (7,4 %).

## Décision

1. `config/config.toml` passe à `a2_logo_prior = true` en pilote.
2. `a2_logo_detector` reste `false` (défaut du code) ; il reste un outil de
   QA en `--report-only`.
3. Aucun changement de code dans `crates/workflow-steps` : le flag active
   le comportement déjà livré par D-052.
4. Le pilote surveille les `apply` A2 créés, les revues « logo source
   uniquement » et les faux applies. Retour à `false` au premier faux apply
   ou si la charge de revue dépasse le gain mesuré.

## Conséquences

- A2 peut créer un `apply` sur un logo source reconnu, même quand l'OCR
  dominant est le texte du logo ; c'est le gain mesuré (`257`).
- Deux revues supplémentaires attendues pour ~248 projets, auto-descriptives
  (« jamais recadré ») et sans action.
- Les 12 autres cibles A2 manquées restent hors de portée : ce sont des
  bandes larges, hautes, sombres, animées ou des sous-titres. Le levier est
  la révision des vetos A2, pas le logo.
- Le veto latéral A1 est une décision séparée, formalisée dans
  `docs/lots/LOT-11-A1-VETO-LATERAL.md` : 7 faux applies pillarbox sur 380,
  veto `gauche ≥ 32 px ou droite ≥ 32 px → review`.

## Conditions de réouverture du détecteur

Ne le réouvrir comme décideur que si une règle aval utilise la confirmation
pour fermer des revues, avec 0 faux noop sur un corpus indépendant contenant
des **bandeaux étroits** (≤ 0,35 de largeur, ≤ 220 px) et au moins 20 clips
négatifs, et si son coût descend à ≤ 0,5 s/projet avec cache.

## Validation

- Étude : 248 projets, 9 359 segments appariés, 92 dossiers ; corpus en
  lecture seule, aucun `.ecproj` modifié.
- Shadow : 248/248 variantes, `rc=0`, 245 audits `pass`, 3 `warn`
  `source_coverage`, 0 `fail`.
- Aucun code de production modifié ; `fmt`, clippy et tests inchangés par
  cette décision.
