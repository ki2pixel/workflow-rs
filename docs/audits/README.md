# Audits, études et preuves

| Champ | Valeur |
| --- | --- |
| Type | Index |
| Statut | Actif |
| Date | 2026-10-26 |
| Portée | Index des 14 rapports de `docs/audits/` et renvois vers les preuves pipeline |
| Voir aussi | [`../README.md`](../README.md), [`../pipeline/VALIDATION-STEP1-8.md`](../pipeline/VALIDATION-STEP1-8.md), [`../../memory-bank/decisionLog.md`](../../memory-bank/decisionLog.md) |

Ce répertoire conserve les rapports datés. Un rapport est une **preuve** : il
décrit ce qui a été mesuré ou décidé à une date donnée, pas nécessairement le
comportement courant. Les constats ne sont pas réécrits lors d'une migration de
chemins ; seules les métadonnées d'en-tête, les chemins et les renvois
`fichier:ligne` peuvent être corrigés.

L'étude interne E1 n'est pas un rapport d'audit : elle est dans
[`../desktop/ETUDE-GUI-NATIVE-LINUX.md`](../desktop/ETUDE-GUI-NATIVE-LINUX.md).

## Vue chronologique

| Date | Rapport | Sujet | Statut |
| --- | --- | --- | --- |
| 2026-10-24 | [`2026-10-24-audit-backend-rust.md`](2026-10-24-audit-backend-rust.md) | Audit backend Rust, constats P0/P1/P2 et plan de remédiation. | Preuve |
| 2026-10-24 | [`2026-10-24-prompt-deep-search-gui-linux.md`](2026-10-24-prompt-deep-search-gui-linux.md) | Prompt autoportant pour le rapport de recherche externe. | Historique |
| 2026-10-24 | [`2026-10-24-confrontation-etudes-gui-linux.md`](2026-10-24-confrontation-etudes-gui-linux.md) | Confrontation E1/E2 et règle de décision Tauri/GTK4. | Preuve |
| 2026-10-24 | [`2026-10-24-analyse-rapport-deep-search-gui-linux.md`](2026-10-24-analyse-rapport-deep-search-gui-linux.md) | Analyse critique du rapport externe et vérifications locales. | Preuve |
| 2026-10-24 | [`Migration-Architecture-GUI-Linux-workflow-rs.md`](Migration-Architecture-GUI-Linux-workflow-rs.md) | ADR-001 externe : étude de faisabilité GUI Linux native. | Historique |
| 2026-10-24 | [`Analyse_Architecturale_GUI_Linux.md`](Analyse_Architecturale_GUI_Linux.md) | Rapport de recherche externe Deep Search Tauri/GTK4. | Historique |
| 2026-10-25 | [`2026-10-25-poc-tauri-j1.md`](2026-10-25-poc-tauri-j1.md) | PoC J1 : scaffold, auth, audit DOM, baseline RSS/PSS/VRAM. | Preuve |
| 2026-10-25 | [`2026-10-25-poc-tauri-j2.md`](2026-10-25-poc-tauri-j2.md) | PoC J2 : stress-test rendu, profils WebKit, canari DMA-BUF. | Preuve |
| 2026-10-25 | [`2026-10-25-poc-tauri-j3.md`](2026-10-25-poc-tauri-j3.md) | PoC J3 : coexistence Tauri + STEP2 CUDA. | Preuve |
| 2026-10-25 | [`2026-10-25-poc-tauri-j4.md`](2026-10-25-poc-tauri-j4.md) | PoC J4 : tray D-Bus, `StatusWatcher` et soak 2 h. | Preuve |
| 2026-10-25 | [`2026-10-25-poc-tauri-j5.md`](2026-10-25-poc-tauri-j5.md) | PoC J5 : paquet `.deb`, supervision systemd, décision A-Strict. | Preuve |
| 2026-10-25 | [`2026-10-25-revue-workflow-desktop-services.md`](2026-10-25-revue-workflow-desktop-services.md) | Revue finale du crate `workflow-desktop-services` avant gel. | Preuve |
| 2026-10-26 | [`2026-10-26-mvp-j1-deb-single-instance-systemd.md`](2026-10-26-mvp-j1-deb-single-instance-systemd.md) | MVP J1 : `.deb`, single-instance D-Bus, cache CUDA systemd. | Preuve |
| 2026-10-26 | [`2026-10-26-mvp-j2-soak-step8-production.md`](2026-10-26-mvp-j2-soak-step8-production.md) | MVP J2 : soak 2 h, STEP8 systemd, bascule production. | Preuve |

## Audits et revues

| Rapport | Rôle |
| --- | --- |
| [`2026-10-24-audit-backend-rust.md`](2026-10-24-audit-backend-rust.md) | Audit statique des neuf crates, tests, lints et contrats ; constats classés P0/P1/P2, audit croisé et remédiation. |
| [`2026-10-25-revue-workflow-desktop-services.md`](2026-10-25-revue-workflow-desktop-services.md) | Revue ligne à ligne du crate partagé, corrections avant gel et contrat de stabilité v0.1.0. |

## Études et documents de recherche

| Rapport | Rôle |
| --- | --- |
| [`Migration-Architecture-GUI-Linux-workflow-rs.md`](Migration-Architecture-GUI-Linux-workflow-rs.md) | Étude externe E2 : architecture et faisabilité d'un client GUI Linux natif. |
| [`Analyse_Architecturale_GUI_Linux.md`](Analyse_Architecturale_GUI_Linux.md) | Rapport externe E3 : veille Tauri/GTK4, WebKitGTK/NVIDIA, sécurité IPC, tray et registre de risques. |
| [`2026-10-24-prompt-deep-search-gui-linux.md`](2026-10-24-prompt-deep-search-gui-linux.md) | Prompt utilisé pour produire E3 ; outil de recherche, pas une preuve. |
| [`2026-10-24-analyse-rapport-deep-search-gui-linux.md`](2026-10-24-analyse-rapport-deep-search-gui-linux.md) | Analyse critique de E3, vérifications `MiniBrowser`, corrections de seuils et PoC révisé. |
| [`2026-10-24-confrontation-etudes-gui-linux.md`](2026-10-24-confrontation-etudes-gui-linux.md) | Confrontation E1/E2, faits vérifiés et recommandation consolidée. |

## PoC Tauri

| Rapport | Rôle |
| --- | --- |
| [`2026-10-25-poc-tauri-j1.md`](2026-10-25-poc-tauri-j1.md) | Scaffold Tauri A-Strict, client HTTP typé, capture du token et baseline. |
| [`2026-10-25-poc-tauri-j2.md`](2026-10-25-poc-tauri-j2.md) | 35 lancements, 125 bascules, quatre profils WebKit et restauration GNOME. |
| [`2026-10-25-poc-tauri-j3.md`](2026-10-25-poc-tauri-j3.md) | Coexistence avec STEP2 CUDA, baseline Firefox, RSS/PSS et VRAM. |
| [`2026-10-25-poc-tauri-j4.md`](2026-10-25-poc-tauri-j4.md) | Tray D-Bus, `StatusWatcher` 1 Hz, matrice d'états et soak d'endurance. |
| [`2026-10-25-poc-tauri-j5.md`](2026-10-25-poc-tauri-j5.md) | `.deb` Tauri, extraction sans root, supervision serveur et décision Tauri A-Strict. |

## MVP desktop

| Rapport | Rôle |
| --- | --- |
| [`2026-10-26-mvp-j1-deb-single-instance-systemd.md`](2026-10-26-mvp-j1-deb-single-instance-systemd.md) | Installation réelle du `.deb`, single-instance `org.workflow.desktop`, focus X11 et cache CUDA. |
| [`2026-10-26-mvp-j2-soak-step8-production.md`](2026-10-26-mvp-j2-soak-step8-production.md) | Étanchéité des secrets, STEP8 sous systemd, soak 2 h et bascule `workflow-server.service`. |

## Preuves pipeline

Ces documents ne sont pas dans `docs/audits/` mais complètent l'index :

| Document | Rôle | Statut |
| --- | --- | --- |
| [`../pipeline/VALIDATION-STEP1-8.md`](../pipeline/VALIDATION-STEP1-8.md) | Chaîne STEP1 → STEP8 sur `1720 Camille.zip`, 8/8 étapes et contrôles d'intégrité. | Preuve |
| [`../pipeline/LOT-1-SHADOW.md`](../pipeline/LOT-1-SHADOW.md) | Validation shadow STEP5/STEP6, 57/57 vérifications identiques. | Preuve |
| [`../pipeline/annexes/STEP2-WINDOWS.md`](../pipeline/annexes/STEP2-WINDOWS.md) | Protocole de validation STEP2 CUDA sous Windows, différé et non exécuté. | Historique |

## Ajouter un rapport

1. Nommer le fichier `AAAA-MM-JJ-sujet.md` dans ce répertoire.
2. Ajouter le cartouche `Type`, `Statut`, `Date`, `Portée`, `Voir aussi`.
3. Ajouter une ligne dans la vue chronologique et dans le groupe approprié.
4. Ajouter le rapport à la section 6 de [`../README.md`](../README.md).
5. Consigner la décision durable dans
   [`../../memory-bank/decisionLog.md`](../../memory-bank/decisionLog.md) si le
   rapport tranche une question d'architecture ou d'exploitation.
