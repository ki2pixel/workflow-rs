# Documentation de workflow-rs

| Champ | Valeur |
| --- | --- |
| Type | Index |
| Statut | Actif |
| Date | 2026-10-26 |
| Portée | Navigation, statuts et règles de maintenance de `docs/` |
| Voir aussi | `../README.md`, `../AGENTS.md`, `../parity/DRIFT.md`, `../memory-bank/activeContext.md` |

Ce répertoire contient la documentation de référence du pipeline Rust. Le
contrat HTTP et les artefacts restent définis par
[`contracts/CONTRAT-ENVIRONNEMENT.md`](contracts/CONTRAT-ENVIRONNEMENT.md) ;
l'état courant du projet est dans
[`../memory-bank/activeContext.md`](../memory-bank/activeContext.md).

> **Un audit ou une validation est une preuve datée, pas un contrat.** En cas de
> conflit, le code, le contrat d'environnement et `parity/DRIFT.md` priment sur
> un rapport d'audit. Les documents `Preuve` et `Historique` ne décrivent pas
> nécessairement le comportement courant.

> **Attention au sens de `docs/`.** Dans les documents de lot, `docs/` sans nom
> de fichier désigne le dossier d'artefacts d'un projet de production
> (`<projet>/docs/`), pas ce répertoire de documentation.

## 1. Comment lire cette documentation

Cinq statuts sont utilisés dans tout le répertoire :

| Statut | Sens |
| --- | --- |
| `Actif` | Fait foi et est appliqué en production ou dans l'exploitation courante. |
| `Prêt` | Procédure validée, pas encore déployée. |
| `Référence` | Spécification, contrat ou bilan de lot stable, consulté pour comprendre et faire évoluer. |
| `Preuve` | Résultat daté d'une validation, d'un audit ou d'un PoC ; non normatif. |
| `Historique` | Conservé pour traçabilité ; ne guide plus les décisions. |

Un document est indexé une seule fois dans sa catégorie principale. Les renvois
croisés signalent les usages secondaires (par exemple une preuve de lot aussi
utile à l'exploitation).

## 2. Contrats & Architecture

| Document | Rôle | Statut |
| --- | --- | --- |
| [`contracts/CONTRAT-ENVIRONNEMENT.md`](contracts/CONTRAT-ENVIRONNEMENT.md) | Référence normative : topologie, routes HTTP, états, schémas JSON/CSV, fichiers et intégrations. | Référence |

Le contrat est complété par [`../parity/DRIFT.md`](../parity/DRIFT.md) pour les
écarts legacy assumés (D-001 à D-027) et par `../AGENTS.md` pour les règles
d'ingénierie. L'architecture du serveur est décrite dans
[`pipeline/LOT-5-ORCHESTRATOR.md`](pipeline/LOT-5-ORCHESTRATOR.md).

## 3. Lots du pipeline

### 3.1 Par étape

| Étape | Document | Lot | Statut |
| --- | --- | --- | --- |
| STEP1 | [`pipeline/LOT-6-STEP1.md`](pipeline/LOT-6-STEP1.md) | Lot 6 | Référence |
| STEP2 | [`pipeline/LOT-2-STEP2.md`](pipeline/LOT-2-STEP2.md) | Lot 2 | Référence |
| STEP3 | [`pipeline/LOT-4-STEP3.md`](pipeline/LOT-4-STEP3.md) | Lot 4 | Référence |
| STEP4 | [`pipeline/LOT-3-STEP4.md`](pipeline/LOT-3-STEP4.md) | Lot 3 | Référence |
| STEP5 | [`pipeline/LOT-1-STEP5.md`](pipeline/LOT-1-STEP5.md) | Lot 1 | Référence |
| STEP6 | [`pipeline/LOT-1-STEP6.md`](pipeline/LOT-1-STEP6.md) | Lot 1 | Référence |
| STEP7 | [`pipeline/LOT-1-STEP7.md`](pipeline/LOT-1-STEP7.md) | Lot 1 | Référence |
| STEP8 | [`pipeline/LOT-1-STEP8.md`](pipeline/LOT-1-STEP8.md) | Lot 1 | Référence |

Chaque fiche décrit le périmètre livré, la parité avec Python, les mesures de
performance et les limites connues.

### 3.2 Lots transverses et validations

| Document | Rôle | Statut |
| --- | --- | --- |
| [`pipeline/LOT-1-SHADOW.md`](pipeline/LOT-1-SHADOW.md) | Validation shadow STEP5/STEP6 sur 19 vidéos et 57/57 vérifications identiques. | Preuve |
| [`pipeline/LOT-1-BRIDGE.md`](pipeline/LOT-1-BRIDGE.md) | Pont After Effects `cuts` et `analyze_manifest` ; disponible, le CEP reste sur Python (D-022). | Référence |
| [`pipeline/LOT-5-ORCHESTRATOR.md`](pipeline/LOT-5-ORCHESTRATOR.md) | Serveur axum, contrat de polling, séquence, arrêt et bascule 5003/5004. | Référence |
| [`pipeline/LOT-7-INTEGRATION.md`](pipeline/LOT-7-INTEGRATION.md) | Séquence réelle STEP2 → STEP8 sur une copie de 244 et rollback contrôlé. | Référence |
| [`pipeline/LOT-8-INGESTION.md`](pipeline/LOT-8-INGESTION.md) | Webhook, normalisation d'URL, historique SQLite, cooldown et téléchargement R2/Dropbox. | Référence |
| [`pipeline/VALIDATION-STEP1-8.md`](pipeline/VALIDATION-STEP1-8.md) | Chaîne complète STEP1 → STEP8 sur `1720 Camille.zip`, 8/8 étapes et 4 `.ecproj`. | Preuve |
| [`pipeline/annexes/STEP2-WINDOWS.md`](pipeline/annexes/STEP2-WINDOWS.md) | Protocole de validation STEP2 CUDA sous Windows, différé et non exécuté (D-022). | Historique |

`LOT-1-SHADOW` et `VALIDATION-STEP1-8` sont aussi des preuves : elles sont
listées ici pour rester dans la série des lots, et cross-référencées dans la
section 6.

## 4. Exploitation & Déploiement

| Document | Rôle | Statut |
| --- | --- | --- |
| [`ops/SWITCH-SYSTEMD.md`](ops/SWITCH-SYSTEMD.md) | Runbook de production : unité `workflow-server.service`, secrets, webhook réel, redémarrage et rollback. | Actif |
| [`ops/SYNC-BACKENDS.md`](ops/SYNC-BACKENDS.md) | Fusion de l'historique, de la liste STEP1 et des archives STEP7 avant une bascule Python ↔ Rust. | Actif |
| [`ops/RETENTION.md`](ops/RETENTION.md) | Politique de rétention des données de travail et cohabitation avec le legacy. | Actif |
| [`ops/SWITCH-PUBLIC.md`](ops/SWITCH-PUBLIC.md) | Bascule publique par reverse proxy Nginx, canari, rollback et routes Python-only ; non déployée. | Prêt |

Ordre de lecture pour une bascule : `SWITCH-SYSTEMD` → `SYNC-BACKENDS` →
`SWITCH-PUBLIC` → `RETENTION`. Ne jamais lancer les moniteurs Python et Rust
en parallèle.

## 5. Interface desktop & packaging

| Document | Rôle | Statut |
| --- | --- | --- |
| [`desktop/PACKAGING-GUI.md`](desktop/PACKAGING-GUI.md) | Construction du `.deb` Tauri, single-instance D-Bus, cache CUDA systemd, supervision et désinstallation. | Actif |
| [`desktop/MVP-FRONTEND-POLLING.md`](desktop/MVP-FRONTEND-POLLING.md) | Suspension opt-in des pollers quand la WebView est masquée, validation J4 et limites. | Référence |
| [`desktop/ETUDE-GUI-NATIVE-LINUX.md`](desktop/ETUDE-GUI-NATIVE-LINUX.md) | Étude de faisabilité d'une interface desktop native ; close par la décision Tauri A-Strict. | Historique |

La décision durable est consignée dans
[`../memory-bank/decisionLog.md`](../memory-bank/decisionLog.md) ; les preuves
d'exécution sont dans [`audits/README.md`](audits/README.md).

## 6. Audits & Preuves

Les rapports datés sont indexés de façon détaillée dans
[`audits/README.md`](audits/README.md). Vue synthétique :

### 6.1 Audits et revues

| Document | Rôle | Statut |
| --- | --- | --- |
| [`audits/2026-10-24-audit-backend-rust.md`](audits/2026-10-24-audit-backend-rust.md) | Audit backend (architecture, concurrence, sécurité, contrats), constats P0/P1/P2 et remédiation. | Preuve |
| [`audits/2026-10-25-revue-workflow-desktop-services.md`](audits/2026-10-25-revue-workflow-desktop-services.md) | Revue finale du crate `workflow-desktop-services` avant gel v0.1.0. | Preuve |

### 6.2 Études GUI et arbitrage

| Document | Rôle | Statut |
| --- | --- | --- |
| [`audits/Migration-Architecture-GUI-Linux-workflow-rs.md`](audits/Migration-Architecture-GUI-Linux-workflow-rs.md) | ADR-001 externe : étude de faisabilité et architecture de migration GUI Linux. | Historique |
| [`audits/Analyse_Architecturale_GUI_Linux.md`](audits/Analyse_Architecturale_GUI_Linux.md) | Rapport de recherche externe Deep Search sur Tauri/GTK4, WebKitGTK/NVIDIA et sécurité IPC. | Historique |
| [`audits/2026-10-24-prompt-deep-search-gui-linux.md`](audits/2026-10-24-prompt-deep-search-gui-linux.md) | Prompt autoportant utilisé pour produire le rapport externe. | Historique |
| [`audits/2026-10-24-analyse-rapport-deep-search-gui-linux.md`](audits/2026-10-24-analyse-rapport-deep-search-gui-linux.md) | Analyse critique du rapport externe et vérifications locales `MiniBrowser`. | Preuve |
| [`audits/2026-10-24-confrontation-etudes-gui-linux.md`](audits/2026-10-24-confrontation-etudes-gui-linux.md) | Confrontation E1/E2, faits vérifiés et règle de décision Tauri/GTK4. | Preuve |

L'étude interne E1 est dans
[`desktop/ETUDE-GUI-NATIVE-LINUX.md`](desktop/ETUDE-GUI-NATIVE-LINUX.md).

### 6.3 PoC Tauri

| Document | Rôle | Statut |
| --- | --- | --- |
| [`audits/2026-10-25-poc-tauri-j1.md`](audits/2026-10-25-poc-tauri-j1.md) | J1 : scaffold, auth par HTML, audit DOM, baseline RSS/PSS/VRAM. | Preuve |
| [`audits/2026-10-25-poc-tauri-j2.md`](audits/2026-10-25-poc-tauri-j2.md) | J2 : 35 lancements, 125 bascules, quatre profils WebKit, canari DMA-BUF. | Preuve |
| [`audits/2026-10-25-poc-tauri-j3.md`](audits/2026-10-25-poc-tauri-j3.md) | J3 : coexistence Tauri + STEP2 CUDA, RSS/PSS et pic VRAM. | Preuve |
| [`audits/2026-10-25-poc-tauri-j4.md`](audits/2026-10-25-poc-tauri-j4.md) | J4 : tray D-Bus, `StatusWatcher` 1 Hz, matrice d'états et soak 2 h. | Preuve |
| [`audits/2026-10-25-poc-tauri-j5.md`](audits/2026-10-25-poc-tauri-j5.md) | J5 : paquet `.deb`, supervision systemd et décision Tauri A-Strict. | Preuve |

### 6.4 MVP desktop

| Document | Rôle | Statut |
| --- | --- | --- |
| [`audits/2026-10-26-mvp-j1-deb-single-instance-systemd.md`](audits/2026-10-26-mvp-j1-deb-single-instance-systemd.md) | MVP J1 : `.deb` réel, single-instance D-Bus, focus X11 et cache CUDA systemd. | Preuve |
| [`audits/2026-10-26-mvp-j2-soak-step8-production.md`](audits/2026-10-26-mvp-j2-soak-step8-production.md) | MVP J2 : étanchéité des secrets, STEP8 systemd, soak 2 h et bascule production. | Preuve |

### 6.5 Preuves pipeline

Les preuves de bout en bout sont décrites dans la section 3.2 :
[`pipeline/VALIDATION-STEP1-8.md`](pipeline/VALIDATION-STEP1-8.md) pour la
chaîne complète, [`pipeline/LOT-1-SHADOW.md`](pipeline/LOT-1-SHADOW.md) pour la
validation shadow et
[`pipeline/annexes/STEP2-WINDOWS.md`](pipeline/annexes/STEP2-WINDOWS.md) pour le
protocole Windows différé.

## 7. Documents voisines

| Document | Rôle |
| --- | --- |
| [`../README.md`](../README.md) | État du projet, commandes, lanceurs et vue d'ensemble. |
| [`../AGENTS.md`](../AGENTS.md) | Règles opérationnelles pour les agents et les développeurs. |
| [`../parity/README.md`](../parity/README.md) | Corpus, commandes et sécurité du harnais de parité. |
| [`../parity/BASELINE.md`](../parity/BASELINE.md) | Mesures de référence Python. |
| [`../parity/DRIFT.md`](../parity/DRIFT.md) | Écarts de comportement legacy et décisions D-xxx. |
| [`../memory-bank/activeContext.md`](../memory-bank/activeContext.md) | État courant et prochain jalon. |
| [`../memory-bank/decisionLog.md`](../memory-bank/decisionLog.md) | Décisions architecturales et techniques durables. |
| [`../memory-bank/progress.md`](../memory-bank/progress.md) | Suivi de progression et points ouverts. |

## 8. Parcours par profil

### Développeur pipeline

1. [`contracts/CONTRAT-ENVIRONNEMENT.md`](contracts/CONTRAT-ENVIRONNEMENT.md)
2. [`pipeline/LOT-5-ORCHESTRATOR.md`](pipeline/LOT-5-ORCHESTRATOR.md)
3. La fiche de l'étape concernée dans le tableau 3.1
4. [`pipeline/VALIDATION-STEP1-8.md`](pipeline/VALIDATION-STEP1-8.md)
5. [`../parity/DRIFT.md`](../parity/DRIFT.md)

### Administrateur système / Ops

1. [`ops/SWITCH-SYSTEMD.md`](ops/SWITCH-SYSTEMD.md)
2. [`ops/SYNC-BACKENDS.md`](ops/SYNC-BACKENDS.md)
3. [`ops/RETENTION.md`](ops/RETENTION.md)
4. [`ops/SWITCH-PUBLIC.md`](ops/SWITCH-PUBLIC.md)
5. [`desktop/PACKAGING-GUI.md`](desktop/PACKAGING-GUI.md)

### Développeur desktop / GUI

1. [`desktop/PACKAGING-GUI.md`](desktop/PACKAGING-GUI.md)
2. [`desktop/MVP-FRONTEND-POLLING.md`](desktop/MVP-FRONTEND-POLLING.md)
3. [`desktop/ETUDE-GUI-NATIVE-LINUX.md`](desktop/ETUDE-GUI-NATIVE-LINUX.md)
4. Les PoC J1 à J5 dans [`audits/README.md`](audits/README.md)
5. [`audits/2026-10-26-mvp-j1-deb-single-instance-systemd.md`](audits/2026-10-26-mvp-j1-deb-single-instance-systemd.md)
   et [`audits/2026-10-26-mvp-j2-soak-step8-production.md`](audits/2026-10-26-mvp-j2-soak-step8-production.md)

### Agent IA

1. [`../AGENTS.md`](../AGENTS.md)
2. Ce hub
3. [`contracts/CONTRAT-ENVIRONNEMENT.md`](contracts/CONTRAT-ENVIRONNEMENT.md)
4. La fiche de lot concernée
5. [`../parity/DRIFT.md`](../parity/DRIFT.md)
6. [`../memory-bank/activeContext.md`](../memory-bank/activeContext.md)

### Auditeur / relecteur

1. Ce hub
2. [`audits/README.md`](audits/README.md)
3. [`audits/2026-10-24-audit-backend-rust.md`](audits/2026-10-24-audit-backend-rust.md)
4. [`audits/2026-10-24-confrontation-etudes-gui-linux.md`](audits/2026-10-24-confrontation-etudes-gui-linux.md)
5. [`../memory-bank/decisionLog.md`](../memory-bank/decisionLog.md)

## 9. Règles de maintenance

- **Un nouveau document va dans un sous-répertoire**, jamais à la racine de
  `docs/`. Seul ce `README.md` vit à la racine.
- **Nommage** : `LOT-N-SUJET.md` dans `pipeline/`, `SWITCH-CIBLE.md` dans
  `ops/`, `MVP-SUJET.md` dans `desktop/`, `AAAA-MM-JJ-sujet.md` dans `audits/`.
- **Indexation obligatoire** : ajouter le document à sa table de ce hub et, si
  c'est une preuve, à `audits/README.md`.
- **Cartouche obligatoire** : `Type`, `Statut`, `Date`, `Portée`, `Voir aussi`.
  La date est celle de la dernière mise à jour de fond, pas celle de la mise en
  forme.
- **Statut unique** : un document n'a qu'un statut parmi `Actif`, `Prêt`,
  `Référence`, `Preuve`, `Historique`. En cas de changement, mettre à jour le
  cartouche et ce hub dans le même commit.
- **Renommage ou déplacement** : réécrire toutes les références avec une
  allowlist, puis vérifier les liens. Ne jamais remplacer `docs/` globalement :
  ce préfixe désigne aussi les artefacts de production.
- **Renvois `fichier:ligne`** : les éviter pour les nouveaux documents ;
  préférer une section nommée. Si un renvoi existant doit être conservé,
  recalculer la ligne après toute modification de l'en-tête.
- **Audits** : ne pas réécrire les constats d'un rapport daté. Seules les
  métadonnées d'en-tête, les chemins et les renvois de ligne peuvent être
  corrigés.

## 10. Correspondance des anciens chemins

<!-- ANCIENS-CHEMINS:DEBUT -->
Tableau historique de la migration du 2026-10-26. Ces chemins ne sont plus
valides ; ils sont conservés pour les liens externes et les vieux commits.

| Ancien chemin (obsolète) | Nouveau chemin |
| --- | --- |
| `docs/CONTRAT-ENVIRONNEMENT.md` | `docs/contracts/CONTRAT-ENVIRONNEMENT.md` |
| `docs/LOT-1-BRIDGE.md` | `docs/pipeline/LOT-1-BRIDGE.md` |
| `docs/LOT-1-SHADOW.md` | `docs/pipeline/LOT-1-SHADOW.md` |
| `docs/LOT-1-STEP5.md` | `docs/pipeline/LOT-1-STEP5.md` |
| `docs/LOT-1-STEP6.md` | `docs/pipeline/LOT-1-STEP6.md` |
| `docs/LOT-1-STEP7.md` | `docs/pipeline/LOT-1-STEP7.md` |
| `docs/LOT-1-STEP8.md` | `docs/pipeline/LOT-1-STEP8.md` |
| `docs/LOT-2-STEP2.md` | `docs/pipeline/LOT-2-STEP2.md` |
| `docs/LOT-3-STEP4.md` | `docs/pipeline/LOT-3-STEP4.md` |
| `docs/LOT-4-STEP3.md` | `docs/pipeline/LOT-4-STEP3.md` |
| `docs/LOT-5-ORCHESTRATOR.md` | `docs/pipeline/LOT-5-ORCHESTRATOR.md` |
| `docs/LOT-6-STEP1.md` | `docs/pipeline/LOT-6-STEP1.md` |
| `docs/LOT-7-INTEGRATION.md` | `docs/pipeline/LOT-7-INTEGRATION.md` |
| `docs/LOT-8-INGESTION.md` | `docs/pipeline/LOT-8-INGESTION.md` |
| `docs/VALIDATION-STEP1-8.md` | `docs/pipeline/VALIDATION-STEP1-8.md` |
| `docs/STEP2-WINDOWS.md` | `docs/pipeline/annexes/STEP2-WINDOWS.md` |
| `docs/SWITCH-PUBLIC.md` | `docs/ops/SWITCH-PUBLIC.md` |
| `docs/SWITCH-SYSTEMD.md` | `docs/ops/SWITCH-SYSTEMD.md` |
| `docs/SYNC-BACKENDS.md` | `docs/ops/SYNC-BACKENDS.md` |
| `docs/RETENTION.md` | `docs/ops/RETENTION.md` |
| `docs/PACKAGING-GUI.md` | `docs/desktop/PACKAGING-GUI.md` |
| `docs/MVP-FRONTEND-POLLING.md` | `docs/desktop/MVP-FRONTEND-POLLING.md` |
| `docs/ETUDE-GUI-NATIVE-LINUX.md` | `docs/desktop/ETUDE-GUI-NATIVE-LINUX.md` |

Les chemins `docs/audits/...` n'ont pas changé.
<!-- ANCIENS-CHEMINS:FIN -->

## 11. État de santé au 2026-10-26

- **37 documents indexés** : 23 documents racine déplacés, 14 rapports d'audit
  conservés.
- **23 chemins déplacés**, **217 références analysées** dans 44 fichiers :
  156 réécrites et 61 vers `docs/audits/` inchangées. **7 renvois
  `fichier:ligne`** recalés.
- **0 orphelin** : le prompt de deep search, auparavant cité par aucun fichier,
  est désormais indexé dans `audits/README.md`.
- **1 référence legacy hors `docs/`** : `assets/effectcraft/MediaSolution.jsx`
  cite `docs/COMPATIBILITY.md`, qui n'existe pas dans ce dépôt. Le fichier JSX
  n'est pas modifié.
- **4 documents `Actif`**, **1 `Prêt`**, **14 `Référence`**, **13 `Preuve`**,
  **5 `Historique`**.
- **Validation** : `cargo fmt --check`, clippy strict et 326 tests workspace
  verts après la migration.
