# Documentation de workflow-rs

| Champ | Valeur |
| --- | --- |
| Type | Index |
| Statut | Actif |
| Date | 2026-10-28 |
| Portée | Navigation, taxonomie et règles de maintenance de `docs/` |
| Voir aussi | `../README.md`, `../AGENTS.md`, `../parity/DRIFT.md`, `../memory-bank/activeContext.md` |

Ce répertoire contient la documentation de référence du pipeline Rust. Le
contrat HTTP et les artefacts restent définis par
[`architecture/CONTRAT-ENVIRONNEMENT.md`](architecture/CONTRAT-ENVIRONNEMENT.md) ;
l'état courant du projet est dans
[`../memory-bank/activeContext.md`](../memory-bank/activeContext.md).

> **Un audit ou une validation est une preuve datée, pas un contrat.** En cas de
> conflit, le code, le contrat d'environnement et `parity/DRIFT.md` priment sur
> un rapport d'audit. Les documents `Preuve` et `Historique` ne décrivent pas
> nécessairement le comportement courant.

> **Attention au sens de `docs/`.** Dans les documents de lot et dans le code,
> `docs/` sans nom de fichier désigne le dossier d'artefacts d'un projet analysé
> (`<projet>/docs/`, `<projet>/docs/post/`), pas ce répertoire. Les chemins
> applicatifs ne sont pas concernés par la taxonomie ci-dessous.

## 1. Taxonomie

| Dossier | Rôle | Statuts dominants |
| --- | --- | --- |
| [`architecture/`](architecture/) | Contrat normatif HTTP, artefacts et intégrations. | Référence |
| [`lots/`](lots/) | Bilans d'implémentation par lot et par étape : périmètre, parité, performance, limites. | Référence, Preuve, Historique |
| [`exploitation/`](exploitation/) | Runbooks de synchronisation, bascule publique et rétention. | Actif, Prêt |
| [`decisions/`](decisions/) | Décisions d'architecture durables (ADR), une par sujet. | Référence |
| [`etudes/`](etudes/) | Études d'opportunité et protocoles de spike, hors production. | Référence, Preuve |
| [`validation/`](validation/) | Validations de bout en bout et audits datés. | Preuve |

Un document est indexé une seule fois dans sa catégorie principale. Les renvois
croisés signalent les usages secondaires, par exemple une validation de lot
aussi utile à l'exploitation.

### Statuts

| Statut | Sens |
| --- | --- |
| `Actif` | Fait foi et est appliqué en production ou dans l'exploitation courante. |
| `Prêt` | Procédure validée, pas encore déployée. |
| `Référence` | Spécification, contrat ou bilan de lot stable, consulté pour comprendre et faire évoluer. |
| `Preuve` | Résultat daté d'une validation, d'un audit ou d'un spike ; non normatif. |
| `Historique` | Conservé pour traçabilité ; ne guide plus les décisions. |

## 2. Par où commencer

| Profil | Ordre de lecture |
| --- | --- |
| Reprise du projet | [`../README.md`](../README.md) → [`../memory-bank/activeContext.md`](../memory-bank/activeContext.md) → cet index. |
| Développeur d'une étape | Fiche `lots/` de l'étape → [`architecture/CONTRAT-ENVIRONNEMENT.md`](architecture/CONTRAT-ENVIRONNEMENT.md) → [`../parity/DRIFT.md`](../parity/DRIFT.md). |
| Opérateur de bascule | [`exploitation/SYNC-BACKENDS.md`](exploitation/SYNC-BACKENDS.md) → [`exploitation/SWITCH-PUBLIC.md`](exploitation/SWITCH-PUBLIC.md) → [`exploitation/RETENTION.md`](exploitation/RETENTION.md). |
| Recherche d'une décision | [`decisions/`](decisions/) → [`../memory-bank/decisionLog.md`](../memory-bank/decisionLog.md) → `parity/DRIFT.md` pour les écarts legacy. |
| Préparation STEP9 | [`etudes/ETUDE-STEP9-AGENTIQUE.md`](etudes/ETUDE-STEP9-AGENTIQUE.md) → [`etudes/SPIKE-STEP9-PROTOCOLE.md`](etudes/SPIKE-STEP9-PROTOCOLE.md) → D-029 dans la memory-bank. |
| Agent IA | `../AGENTS.md` → cet index → `.agents/rules/` → fiche `lots/` concernée. |

## 3. Architecture et contrat

| Document | Rôle | Statut |
| --- | --- | --- |
| [`architecture/CONTRAT-ENVIRONNEMENT.md`](architecture/CONTRAT-ENVIRONNEMENT.md) | Référence normative : topologie, routes HTTP, états, schémas JSON/CSV, fichiers et intégrations. | Référence |

Le contrat est complété par [`../parity/DRIFT.md`](../parity/DRIFT.md) pour les
écarts legacy assumés et par `../AGENTS.md` pour les règles d'ingénierie.

## 4. Lots du pipeline

### 4.1 Par étape

| Étape | Document | Lot | Statut |
| --- | --- | --- | --- |
| STEP1 | [`lots/LOT-6-STEP1.md`](lots/LOT-6-STEP1.md) | Lot 6 | Référence |
| STEP2 | [`lots/LOT-2-STEP2.md`](lots/LOT-2-STEP2.md) | Lot 2 | Référence |
| STEP3 | [`lots/LOT-4-STEP3.md`](lots/LOT-4-STEP3.md) | Lot 4 | Référence |
| STEP4 | [`lots/LOT-3-STEP4.md`](lots/LOT-3-STEP4.md) | Lot 3 | Référence |
| STEP5 | [`lots/LOT-1-STEP5.md`](lots/LOT-1-STEP5.md) | Lot 1 | Référence |
| STEP6 | [`lots/LOT-1-STEP6.md`](lots/LOT-1-STEP6.md) | Lot 1 | Référence |
| STEP7 | [`lots/LOT-1-STEP7.md`](lots/LOT-1-STEP7.md) | Lot 1 | Référence |
| STEP8 | [`lots/LOT-1-STEP8.md`](lots/LOT-1-STEP8.md) | Lot 1 | Référence |
| A5/A6 (post-STEP8) | [`lots/LOT-9-POST-STEP8.md`](lots/LOT-9-POST-STEP8.md) | Lot 9 | Référence |

La numérotation des lots ne suit pas celle des étapes : `LOT-3-STEP4` et
`LOT-4-STEP3` sont volontairement dans cet ordre historique.

### 4.2 Lots transverses

| Document | Rôle | Statut |
| --- | --- | --- |
| [`lots/LOT-1-BRIDGE.md`](lots/LOT-1-BRIDGE.md) | Pont After Effects `cuts` et `analyze_manifest` ; disponible, le panneau CEP reste sur Python (D-022). | Référence |
| [`lots/LOT-1-SHADOW.md`](lots/LOT-1-SHADOW.md) | Validation shadow STEP5/STEP6 sur 19 vidéos et 20 projets. | Preuve |
| [`lots/LOT-5-ORCHESTRATOR.md`](lots/LOT-5-ORCHESTRATOR.md) | Serveur axum, contrat de polling, séquence, arrêt et bascule 5003/5004. | Référence |
| [`lots/LOT-7-INTEGRATION.md`](lots/LOT-7-INTEGRATION.md) | Séquence réelle STEP2 → STEP8 et rollback contrôlé. | Référence |
| [`lots/LOT-8-INGESTION.md`](lots/LOT-8-INGESTION.md) | Webhook, normalisation d'URL, historique SQLite, cooldown et téléchargement R2/Dropbox. | Référence |
| [`lots/STEP2-WINDOWS.md`](lots/STEP2-WINDOWS.md) | Protocole de validation STEP2 CUDA sous Windows, différé et non exécuté (D-022). | Historique |

## 5. Exploitation

| Document | Rôle | Statut |
| --- | --- | --- |
| [`exploitation/SYNC-BACKENDS.md`](exploitation/SYNC-BACKENDS.md) | Fusion de l'historique, de la liste STEP1 et des archives STEP7 avant une bascule Python ↔ Rust. | Actif |
| [`exploitation/SWITCH-PUBLIC.md`](exploitation/SWITCH-PUBLIC.md) | Bascule publique par reverse proxy Nginx, canari, rollback et routes Python-only ; non déployée. | Prêt |
| [`exploitation/RETENTION.md`](exploitation/RETENTION.md) | Politique de rétention des données de travail et cohabitation avec le legacy. | Actif |

Ordre de lecture pour une bascule : `SYNC-BACKENDS` → `SWITCH-PUBLIC` →
`RETENTION`. Ne jamais lancer les moniteurs Python et Rust en parallèle.

## 6. Décisions

| Document | Rôle | Statut |
| --- | --- | --- |
| [`decisions/DECISION-ABANDON-TAURI.md`](decisions/DECISION-ABANDON-TAURI.md) | D-028 : abandon du client desktop Tauri, périmètre retiré et procédure de restauration. | Référence |

Toute nouvelle décision durable suit la convention
`decisions/DECISION-<SUJET>.md`, avec une entrée datée dans
[`../memory-bank/decisionLog.md`](../memory-bank/decisionLog.md).

## 7. Études

| Document | Rôle | Statut |
| --- | --- | --- |
| [`etudes/ETUDE-STEP9-AGENTIQUE.md`](etudes/ETUDE-STEP9-AGENTIQUE.md) | Opportunité et faisabilité du post-STEP8 : recadrage 9:16, nettoyage, habillage, rendu, scénarios déterministe, QA agentique et co-monteur. | Référence |
| [`etudes/SPIKE-STEP9-PROTOCOLE.md`](etudes/SPIKE-STEP9-PROTOCOLE.md) | Protocole de spike isolé, corpus, métriques, critères d'acceptation et d'arrêt. | Preuve |

Ces documents ne décrivent pas la production courante : la décision D-029
maintient la consolidation déterministe avant tout STEP9 agentique.

## 8. Validation et audits

| Document | Rôle | Statut |
| --- | --- | --- |
| [`validation/VALIDATION-STEP1-8.md`](validation/VALIDATION-STEP1-8.md) | Chaîne complète STEP1 → STEP8 sur `1720 Camille.zip`, isolation et contrôles d'intégrité. | Preuve |
| [`validation/audits/2026-10-24-audit-backend-rust.md`](validation/audits/2026-10-24-audit-backend-rust.md) | Audit backend : architecture, concurrence, sécurité, contrats, constats P0/P1/P2 et remédiation. | Preuve |

Les audits futurs suivent la convention
`validation/audits/AAAA-MM-JJ-<sujet>.md`. Un audit est une photographie datée :
il ne remplace ni le contrat ni `parity/DRIFT.md`.

## 9. Conventions de rédaction

- Les renvois entre documents utilisent des chemins **repo-relatifs** en
  backticks, par exemple `docs/lots/LOT-1-STEP8.md`. Ils restent valides quel
  que soit le fichier courant et sont vérifiables mécaniquement.
- Les liens Markdown relatifs sont réservés à cet index, où ils sont relatifs à
  `docs/`.
- `docs/` sans nom de fichier désigne toujours un dossier d'artefacts de projet,
  jamais ce répertoire.
- Un nouveau document va dans le dossier de sa catégorie, sans renommer les
  fichiers existants, et gagne une ligne dans cet index.
- Les documents sont en français ; les commits restent en anglais.

## 10. Correspondance ancien → nouveau

Cette table facilite la transition depuis l'ancienne organisation à plat. Les
chemins de la colonne « Avant » ne sont plus valides.

| Avant | Après |
| --- | --- |
| `docs/CONTRAT-ENVIRONNEMENT.md` | [`architecture/CONTRAT-ENVIRONNEMENT.md`](architecture/CONTRAT-ENVIRONNEMENT.md) |
| `docs/DECISION-ABANDON-TAURI.md` | [`decisions/DECISION-ABANDON-TAURI.md`](decisions/DECISION-ABANDON-TAURI.md) |
| `docs/RETENTION.md` | [`exploitation/RETENTION.md`](exploitation/RETENTION.md) |
| `docs/SWITCH-PUBLIC.md` | [`exploitation/SWITCH-PUBLIC.md`](exploitation/SWITCH-PUBLIC.md) |
| `docs/SYNC-BACKENDS.md` | [`exploitation/SYNC-BACKENDS.md`](exploitation/SYNC-BACKENDS.md) |
| `docs/ETUDE-STEP9-AGENTIQUE.md` | [`etudes/ETUDE-STEP9-AGENTIQUE.md`](etudes/ETUDE-STEP9-AGENTIQUE.md) |
| `docs/SPIKE-STEP9-PROTOCOLE.md` | [`etudes/SPIKE-STEP9-PROTOCOLE.md`](etudes/SPIKE-STEP9-PROTOCOLE.md) |
| `docs/LOT-1-BRIDGE.md` | [`lots/LOT-1-BRIDGE.md`](lots/LOT-1-BRIDGE.md) |
| `docs/LOT-1-SHADOW.md` | [`lots/LOT-1-SHADOW.md`](lots/LOT-1-SHADOW.md) |
| `docs/LOT-1-STEP5.md` | [`lots/LOT-1-STEP5.md`](lots/LOT-1-STEP5.md) |
| `docs/LOT-1-STEP6.md` | [`lots/LOT-1-STEP6.md`](lots/LOT-1-STEP6.md) |
| `docs/LOT-1-STEP7.md` | [`lots/LOT-1-STEP7.md`](lots/LOT-1-STEP7.md) |
| `docs/LOT-1-STEP8.md` | [`lots/LOT-1-STEP8.md`](lots/LOT-1-STEP8.md) |
| `docs/LOT-2-STEP2.md` | [`lots/LOT-2-STEP2.md`](lots/LOT-2-STEP2.md) |
| `docs/LOT-3-STEP4.md` | [`lots/LOT-3-STEP4.md`](lots/LOT-3-STEP4.md) |
| `docs/LOT-4-STEP3.md` | [`lots/LOT-4-STEP3.md`](lots/LOT-4-STEP3.md) |
| `docs/LOT-5-ORCHESTRATOR.md` | [`lots/LOT-5-ORCHESTRATOR.md`](lots/LOT-5-ORCHESTRATOR.md) |
| `docs/LOT-6-STEP1.md` | [`lots/LOT-6-STEP1.md`](lots/LOT-6-STEP1.md) |
| `docs/LOT-7-INTEGRATION.md` | [`lots/LOT-7-INTEGRATION.md`](lots/LOT-7-INTEGRATION.md) |
| `docs/LOT-8-INGESTION.md` | [`lots/LOT-8-INGESTION.md`](lots/LOT-8-INGESTION.md) |
| `docs/LOT-9-POST-STEP8.md` | [`lots/LOT-9-POST-STEP8.md`](lots/LOT-9-POST-STEP8.md) |
| `docs/STEP2-WINDOWS.md` | [`lots/STEP2-WINDOWS.md`](lots/STEP2-WINDOWS.md) |
| `docs/VALIDATION-STEP1-8.md` | [`validation/VALIDATION-STEP1-8.md`](validation/VALIDATION-STEP1-8.md) |
| `docs/audits/2026-10-24-audit-backend-rust.md` | [`validation/audits/2026-10-24-audit-backend-rust.md`](validation/audits/2026-10-24-audit-backend-rust.md) |
