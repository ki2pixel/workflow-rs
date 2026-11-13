# Lot 12.2 — banque centrale de repli A7

> Statut : **implémenté et validé le 2026-11-12** (D-066) ; résilience des
> assets locaux corrompus validée le 2026-11-13 (D-069).
> Date : 2026-11-12.
> Décisions liées : D-064 (A7), D-065 (parseur, alias, `tail_overlay`), D-069
> (repli sur asset local corrompu).
> Code : `crates/workflow-core/src/config.rs`,
> `crates/workflow-core/src/paths.rs`,
> `crates/workflow-core/src/mission.rs`,
> `crates/workflow-steps/src/post/a7.rs`,
> `crates/workflow-steps/src/post/run.rs`,
> `config/config.toml`, `.gitignore`.

## Problème

Le Lot 12.1 résolvait les logos dans `<projet>/docs`, le dossier alimenté par
le client. Quand un email demande un logo que le client n'a pas joint, A7
partait en `review` même si l'asset existait dans la banque de référence
Média Solution. Le repli devait rester **additif et optionnel** : un projet
sans banque, une banque absente ou un asset non conforme ne doivent jamais
bloquer le pipeline.

## Contrat

### Configuration

```toml
[paths]
mission_overlays = "assets/mission_overlays"
```

- Champ `Option<PathBuf>` dans `PathsConfig`, vide/absent par défaut.
- `PathsConfig::resolve()` filtre une valeur vide et expose
  `ResolvedPaths.mission_overlays`.
- `config check` échoue si le champ est configuré mais que le répertoire
  n'existe pas. Une valeur vide désactive le repli.
- Le serveur travaille dans `/home/kidpixel/workflow-rs` ; le chemin relatif
  est donc stable pour les lancements serveur et les commandes manuelles.

### Ordre de résolution

1. `docs/` local, fichier exact ;
2. `docs/` local, nom normalisé (casse, accents, ponctuation) ;
3. banque centrale, fichier exact ;
4. banque centrale, nom normalisé ;
5. extension implicite `.png` puis `.mov`, local puis banque ;
6. table d'alias historique ;
7. `known_absent` (`lphdf.png`, `edc.png`) ;
8. `review` sans insertion.

Une ambiguïté locale (deux formats pour `MSL1` sans extension) reste une
ambiguïté : la banque n'est pas consultée pour la masquer. Un asset local
valide gagne toujours sur un asset central du même nom.

### Provenance et validation

- `LogoProvenance` vaut `local_docs` ou `central_fallback`.
- Chaque asset retenu passe `probe_asset` : PNG 1080×1920 avec alpha, MOV
  compatible 1080×1920 avec durée et audio.
- `A7Added.source` et le sidecar `schema: 2` portent la provenance réelle.
- Un asset central non conforme reste en `review` : aucun placement partiel.

### Résilience d'un asset local corrompu (D-069)

Un asset présent dans `docs/` mais illisible ne doit plus masquer son homologue
central. Exemple réel : `183 Camille/docs/LFAUIT.png`, 15 octets ASCII
(`logo de la base`) sans signature PNG.

1. `resolve_target` renvoie d'abord `LocalDocs` comme avant ;
2. `probe_asset` échoue (`asset_unreadable`) ou le profil est invalide
   (`asset_dimensions`) ;
3. A7 appelle `LogoResolver::resolve_fallback_target`, qui réinterroge
   uniquement la banque (même normalisation, extension implicite et table
   d'alias) ;
4. si le repli passe la sonde, l'asset est inséré avec
   `source = "central_fallback"` et le sidecar reçoit un warning additif
   `local_asset_corrupted_fallback_used` (`code`/`line`/`detail`), sans
   changer `reasons` ni `missing` ; la décision reste `apply` si le reste de
   la mission est satisfait ;
5. si le repli échoue aussi, l'erreur locale initiale est conservée et son
   détail mentionne l'échec central ; la mission reste en `review`.

Le repli n'est tenté que depuis `LocalDocs` : un asset central déjà invalide ne
déclenche aucune seconde tentative ni boucle. Le projet brut et les assets de
`docs/` ne sont jamais modifiés ; le sidecar A7 est le seul ajout, et seul
`_assist.ecproj` est réparé (D-064, D-022).

### Dégradation

- Banque non configurée (`mission_overlays` absent ou vide) : comportement
  D-065 inchangé.
- Banque configurée mais introuvable au runtime : elle est ignorée, le repli
  local continue. `config check` la signale séparément.
- Asset absent des deux sources : `review` (`missing_asset`), exit 0, aucun
  calque.
- Un chemin relatif de banque est canonicalisé avant le calcul du chemin
  `.ecproj`; le projet référence donc `../../assets/mission_overlays/...` et
  reste portable.

## Banque déployée

`assets/mission_overlays/` contient les **117 assets réels** :

- 109 PNG 1080×1920 RGBA (versionnés) ;
- 8 MOV ProRes 4444 `yuva444p12le` 1080×1920 (2,2 Go, gitignorés).

`manifest.toml` fige pour chaque asset le nom, le type, les dimensions, la
durée MOV, le codec, le pixel format, la présence audio, la taille et le
SHA-256. `README.md` documente le déploiement, l'ordre de résolution, les
chemins `.ecproj` et les limites.

`.gitignore` exclut uniquement `/assets/mission_overlays/*.mov` ; les PNG, le
manifeste et le README restent versionnés. Un checkout neuf doit recopier les
MOV depuis la source client avant d'utiliser un repli MOV.

## Implémentation

- `config.rs` : champ `mission_overlays`, défaut `None`, validation
  `is_dir()`, test de typage et de chemin vide.
- `paths.rs` : `ResolvedPaths.mission_overlays`, filtrage des valeurs vides.
- `mission.rs` : `LogoProvenance`, `LogoTarget`,
  `LogoResolver::with_fallback`, `resolve_target`,
  `resolve_sequence_target`. La résolution locale reste l'API historique
  (`LogoResolution`, `resolve`, `resolve_sequence`). Depuis D-069,
  `resolve_fallback_target` interroge la seule banque pour les reprises A7.
- `a7.rs` : `A7Options.mission_overlays`, repli canonicalisé, provenance dans
  `Asset` et `A7Added`, message `missing_asset` mentionnant les deux sources.
  Depuis D-069, `resolve_assets` re-sonde la banque après un échec local et
  ajoute le warning `local_asset_corrupted_fallback_used` au sidecar.
- `run.rs` : `PostOptions.mission_overlays`, alimenté par
  `PathsConfig::resolve()`, transmis à A7.
- `config/config.toml` : `mission_overlays = "assets/mission_overlays"`.

## Validation

- `cargo test -p workflow-core mission --lib` : 29 tests, dont priorité
  local/banque, provenance, alias central, extension implicite,
  `known_absent` et repli dédié sur asset local corrompu.
- `cargo test -p workflow-steps post::a7 --lib` : 22 tests, dont banque
  centrale seule (`source: central_fallback`, chemin
  `../../assets/mission_overlays/central.png`), priorité locale, banque
  absente → `review`, asset central non conforme (8×8) rejeté et reprise
  après corruption locale.
- `post::run::tests::post_options_carry_mission_overlays_from_config` :
  propagation de la configuration.
- `config check` sur `config/config.toml` : valide avec la banque déployée.
- Suite workspace : 661 tests listés (659 exécutés + 2 ignorés), `fmt` et
  `clippy` verts (D-066).
- Aucun `.ecproj`, `runtime/` ou `/mnt/cache` modifié.

### Résilience asset local corrompu (2026-11-13, D-069)

- **Échantillon réel** : `docs/LFAUIT.png` extrait de
  `183 Camille_brut.zip` sous `/tmp/opencode` = 15 octets ASCII, sans signature
  PNG (SHA-256 `52ce014e…b063a4`). Homologue central
  `assets/mission_overlays/LFAUIT.png` = 35 582 octets, 1080×1920 RGBA,
  SHA-256 `cc9a036f…d4e10a`, conforme au manifeste.
- **Tests** : `fallback_target_bypasses_a_corrupt_local_asset` (workflow-core),
  `corrupt_local_asset_falls_back_to_central_bank` (apply, ajout au chutier,
  calque, chemin `../../assets/mission_overlays/LFAUIT.png`, warning),
  `invalid_local_profile_falls_back_to_central_bank` (PNG 8×8 local),
  `corrupt_local_asset_without_fallback_degrades_to_review` (banque vide →
  `review` / `missing_asset` / `asset_unreadable`, aucun calque),
  `corrupt_fallback_keeps_local_error_and_reviews` (repli central corrompu,
  erreur locale conservée).
- **Suite workspace** : 675 tests listés (673 exécutés + 2 ignorés), 0 échec ;
  `fmt` et `clippy -D warnings` verts. Aucun `.ecproj`, `runtime/` ou
  `/mnt/cache` modifié.

## Limites

- Les MOV de la banque ne sont pas versionnés ; sans copie locale, un repli
  MOV part en `review`.
- La sonde PNG lit l'en-tête, les dimensions et le canal alpha ; elle ne
  décode pas toute l'image. Un PNG dont l'en-tête est valide mais dont les
  données IDAT sont tronquées peut donc passer la sonde.
- Le projet brut et `docs/` ne sont pas réparés : l'import `MediaSolution.jsx`
  du brut peut encore échouer sur l'asset corrompu, seul `_assist.ecproj`
  bénéficie du repli (D-069).
- La banque n'est pas un catalogue indexé : elle est relue au démarrage de
  chaque résolution A7 (117 fichiers, coût négligeable).
- Aucun alias n'est déduit de la présence d'un fichier : la table reste
  fermée.
- Le masque QA MOV et le benchmark stratifié complet restent le Lot 12.3.

## Références

- D-064, D-065, D-066, D-069 dans `memory-bank/decisionLog.md`.
- `docs/lots/LOT-12-A7-MISSIONS.md`.
- `docs/lots/LOT-12.1-PARSEUR-ALIAS-TAIL.md`.
- `assets/mission_overlays/README.md`.
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md`.
