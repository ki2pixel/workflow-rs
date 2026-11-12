# Lot 12.2 — banque centrale de repli A7

> Statut : **implémenté et validé le 2026-11-12** (D-066).
> Date : 2026-11-12.
> Décisions liées : D-064 (A7), D-065 (parseur, alias, `tail_overlay`).
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
  (`LogoResolution`, `resolve`, `resolve_sequence`).
- `a7.rs` : `A7Options.mission_overlays`, repli canonicalisé, provenance dans
  `Asset` et `A7Added`, message `missing_asset` mentionnant les deux sources.
- `run.rs` : `PostOptions.mission_overlays`, alimenté par
  `PathsConfig::resolve()`, transmis à A7.
- `config/config.toml` : `mission_overlays = "assets/mission_overlays"`.

## Validation

- `cargo test -p workflow-core mission --lib` : 27 tests, dont priorité
  local/banque, provenance, alias central, extension implicite et
  `known_absent`.
- `cargo test -p workflow-steps post::a7 --lib` : 18 tests, dont banque
  centrale seule (`source: central_fallback`, chemin
  `../../assets/mission_overlays/central.png`), priorité locale, banque
  absente → `review` et asset central non conforme (8×8) rejeté.
- `post::run::tests::post_options_carry_mission_overlays_from_config` :
  propagation de la configuration.
- `config check` sur `config/config.toml` : valide avec la banque déployée.
- Suite workspace : 661 tests listés (659 exécutés + 2 ignorés), `fmt` et
  `clippy` verts (D-066).
- Aucun `.ecproj`, `runtime/` ou `/mnt/cache` modifié.

## Limites

- Les MOV de la banque ne sont pas versionnés ; sans copie locale, un repli
  MOV part en `review`.
- La banque n'est pas un catalogue indexé : elle est relue au démarrage de
  chaque résolution A7 (117 fichiers, coût négligeable).
- Aucun alias n'est déduit de la présence d'un fichier : la table reste
  fermée.
- Le masque QA MOV et le benchmark stratifié complet restent le Lot 12.3.

## Références

- D-064, D-065, D-066 dans `memory-bank/decisionLog.md`.
- `docs/lots/LOT-12-A7-MISSIONS.md`.
- `docs/lots/LOT-12.1-PARSEUR-ALIAS-TAIL.md`.
- `assets/mission_overlays/README.md`.
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md`.
