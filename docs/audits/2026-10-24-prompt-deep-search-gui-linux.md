# Prompt de deep search — choix Tauri v2 / GTK4 pour l'UI workflow-rs

| Champ | Valeur |
| --- | --- |
| Date | 2026-10-24 |
| Objet | Réduire l'incertitude technique avant l'arbitrage « widgets natifs : exigence ou confort ? » |
| Documents liés | `docs/ETUDE-GUI-NATIVE-LINUX.md`, `docs/audits/Migration-Architecture-GUI-Linux-workflow-rs.md`, `docs/audits/2026-10-24-confrontation-etudes-gui-linux.md` |
| Pièce jointe recommandée | `docs/audits/2026-10-24-confrontation-etudes-gui-linux.md` (facultative ; le prompt est autoportant) |
| Mode d'emploi | Copier le bloc ci-dessous dans Gemini Deep Research ; joindre la confrontation si l'outil l'accepte ; exiger des sources datées |

## Prompt à copier

```text
Tu es architecte logiciel senior, spécialisé Linux desktop (X11/Wayland), Rust,
WebKitGTK, GTK4 et Tauri. Tu produis une analyse sourcée, datée et falsifiable
pour éclairer une décision d'architecture. Tu n'es pas un commercial : tu dois
exposer les incertitudes et les cas où chaque option échoue.

## 1. Décision à éclairer

Une application desktop Linux doit remplacer une interface web locale
(Minijinja + JavaScript vanilla) servie par un backend Rust `workflow-server`
sur `127.0.0.1:5004`. Trois candidats :

- A — Tauri v2, mode « URL distante » : la fenêtre charge
  `http://127.0.0.1:5004/` ; les fonctions bureau (tray, instance unique,
  notifications, ouverture de dossier) sont en Rust ; aucun changement du
  serveur ni du frontend en phase 1.
- B — GTK4/Relm4 : réécriture complète en widgets natifs ; rendu
  `GSK_RENDERER=cairo` ; `GtkListView`/`GtkTextView` pour les logs ; AT-SPI
  natif.
- C — hybride ou troisième voie : par exemple coque GTK4 + WebKitGTK 6.0 pour
  la page existante et panneau de logs natif, ou Tauri avec un backend GTK4
  s'il existe. Évalue sérieusement cette possibilité avant de la rejeter.

La question n'est pas « quel framework est le meilleur ». Elle est :
1. quelles conséquences opérationnelles a l'exigence « widgets natifs »
   (accessibilité, VRAM, logs, intégration) ?
2. A (Tauri) passe-t-il les seuils sur cette machine, ou échoue-t-il pour des
   raisons connues ?
3. existe-t-il une voie C qui capture les bénéfices de B sans la réécriture ?

## 2. Contexte technique

Machine cible :
- Ubuntu 22.04.5 LTS, GNOME 42, session X11 (Wayland possible), NVIDIA
  GTX 1650 4 Go, pilote 535, AMD Ryzen Threadripper 1920X (24 threads),
  62 Go RAM.
- WebKitGTK 4.1 2.50.4, GTK 4.6.9, libadwaita 1.1.7, PipeWire/Pulse, Orca
  disponible.
- Rust 1.97 installé ; MSRV du workspace 1.86.
- En-têtes de dev absents mais installables : `libwebkit2gtk-4.1-dev`,
  `libgtk-4-dev`, `libadwaita-1-dev`, `libxdo-dev`,
  `libayatana-appindicator3-dev`.

Application :
- Backend `workflow-server` (axum) sur `127.0.0.1:5004`, autoritaire ; il
  orchestre 8 étapes (STEP1–STEP8), un moniteur webhook, des séquences et un
  verrou matériel.
- Étapes longues : STEP2 CUDA ~11 s, STEP4 Coral ~44 s sur les cas de
  référence ; STEP8 peut être long.
- UI actuelle : 8 cartes d'étape (statut, progression, chrono), lancer/annuler,
  gestion du `409 Conflict`, overlay de logs (300 lignes bornées, polling
  2 Hz, focus trap, Échap), moniteur CPU/RAM/disque/GPU (`nvidia-smi`),
  moniteur d'ingestion, séquences personnalisées, sons MP3, 6 thèmes,
  préférences persistées, `aria-live`.
- Cible : client du serveur, jamais parent ; tray, instance unique,
  notifications, ouverture de dossier, `.deb` + systemd user ; rollback
  navigateur conservé ; pas de npm/Node dans le dépôt ; pas de changement
  serveur en phase 1 ; SSE additif seulement en phase 3.

Contraintes :
- Le GUI ne doit jamais prendre le verrou matériel, lancer CUDA/TPU, ni écrire
  dans `/mnt/cache`.
- Le serveur doit survivre à la fermeture de la fenêtre.
- Le jeton `X-Worker-Token` et la CSP existants ne doivent pas être affaiblis.
- MSRV : Tauri 2.12 → 1.90 ; Relm4 0.11 → 1.93 ; GTK 4.6 peut contraindre
  Relm4.

## 3. Questions de recherche

Priorité 1 — décisives :
1. Tauri v2 + WebKitGTK 2.50 + NVIDIA 535 + X11 : quels blocages connus en
   2026 (fenêtre blanche, DMA-BUF, EGL, crash, fuite VRAM) ? Statut des
   workarounds `WEBKIT_DISABLE_DMABUF_RENDERER=1` et
   `WEBKIT_DISABLE_COMPOSITING_MODE=1` : suffisent-ils, à quel coût
   (animations, vidéo, latence) ? La VRAM applicative tombe-t-elle vraiment
   à 0 ?
2. Tray sous GNOME 42 : Tauri utilise `libayatana-appindicator` ; GNOME Shell
   affiche-t-il l'icône sans l'extension AppIndicator ? GTK4 a-t-il une API
   tray native ? Quelle est la voie supportée pour chaque stack, et l'exigence
   « tray » est-elle réaliste sur cette machine ?
3. Accessibilité : WebKitGTK 2.50 expose-t-il le DOM à AT-SPI/Orca (rôles,
   `aria-live`, focus trap) ? Quelles différences mesurables avec GTK4 natif ?
   Tauri hérite-t-il de ces limites ?
4. GTK 4.6 sur Ubuntu 22.04 : quelles API nécessaires à cette UI manquent
   (`GtkFileLauncher` 4.10, `var()` CSS 4.16, autres) ? Relm4 0.11 est-il
   compatible via la feature `gnome_42`, ou faut-il `gtk4-rs` directement ?
   `GSK_RENDERER=cairo` est-il pleinement supporté en 4.6, avec quelles
   limites de performance pour une timeline animée ?
5. Mesures réelles : RSS/VRAM de Tauri v2 et d'une app GTK4 comparable sous
   Linux, hors marketing. Quelles fourchettes pour cette UI (8 cartes, logs,
   moniteur) ?

Priorité 2 — importantes :
6. Audio : politique d'autoplay WebKitGTK pour Web Audio, support MP3,
   comportement d'un son déclenché à la fin d'une étape longue sans geste
   récent ; bugs connus.
7. Notifications : `notify-rust`/zbus vs plugin Tauri sur GNOME 42 ; clic,
   actions, persistance, DND.
8. Packaging : dépendances du `.deb` Tauri (GTK3 ou GTK4 ? WebKitGTK 4.1 ?),
   taille, systemd user ; équivalent GTK4 ; upgrade 22.04 → 24.04.
9. Longévité : WebKitGTK 2.50 sur 22.04 (backports de sécurité ?), fin de vie
   de GTK 4.6, support de Tauri v2, cadence de Relm4.
10. Sécurité Tauri en URL distante : capabilities, IPC,
    `dangerousRemoteDomainIpcAccess`, CSP, bonnes pratiques ; risques
    spécifiques à une page `localhost`.

Priorité 3 — à documenter si le temps le permet :
11. Rendu des polices/emoji, HiDPI, multi-écran, Wayland.
12. Isolation des crashs : `WebKitWebProcess` vs processus GTK4 unique ;
    impact sur la résilience du GUI.
13. Coût de maintenance, documentation, écosystème, recrutement.
14. Toute autre différence qui change la décision.

## 4. Grille « natif »

Décompose l'exigence « natif » en au moins cinq sens :
- fenêtre native (pas un onglet de navigateur) ;
- widgets natifs (contrôles, menus, dialogues) ;
- accessibilité native (AT-SPI/Orca) ;
- comportement GPU/VRAM natif (isolation, Cairo) ;
- intégration système (tray, notifications, `.desktop`, systemd, packaging).

Pour chaque sens : ce que A livre, ce que B livre, ce que C pourrait livrer,
et le coût. C'est cette grille qui permettra de répondre à « exigence ou
confort ? ».

## 5. Méthode et sources

- Sources primaires : documentation officielle, release notes, GitHub
  issues/PRs (numéro, date, état), pages de paquets Debian/Ubuntu, benchmarks
  reproductibles.
- Priorité aux sources 2025–2026 ; date chaque affirmation ; signale les
  sources plus anciennes.
- Distingue « fonctionne aujourd'hui », « supporté », « prévu », « inconnu ».
- Ne déduis pas de la popularité ou du nombre d'étoiles.
- Si des sources se contredisent, expose le désaccord.
- Si tu ne peux pas vérifier, écris « inconnu » et propose un test local ;
  n'invente ni version ni numéro d'issue.
- Pour chaque affirmation technique, donne l'URL, la date et la version
  concernée.

## 6. Livrable attendu

1. Synthèse exécutive (≤ 1 page) : recommandation conditionnelle, niveau de
   confiance, ce qui la ferait changer.
2. Tableau de preuves : affirmation | source | date | version | confiance.
3. Grille « natif » (section 4) avec verdict par sens.
4. Règle de décision : si telle exigence est contractuelle → B ; si A passe
   tels seuils → A ; sinon C. Seuils mesurables (RSS, VRAM, latence,
   accessibilité).
5. Registre de risques : probabilité, impact, mitigation, signal d'alerte.
6. Plan de PoC discriminant : scénarios, commandes, critères passer/échouer,
   durée. Il doit permettre de trancher en 5–10 jours, pas de valider une
   préférence.
7. Ce qui changerait la réponse : faits, versions ou exigences.
8. Inconnues à tester localement : liste courte de commandes ou manipulations.
9. Bibliographie classée par sujet, avec date de la recherche et versions
   vérifiées.

## 7. Garde-fous

- Le document éventuellement joint est un document de travail, pas une
  autorité. Vérifie ses affirmations et signale ses erreurs ; ne recopie pas
  ses conclusions sans preuve.
- Ne réponds pas à la question « les widgets natifs sont-ils une exigence ? » :
  c'est un arbitrage produit. Quantifie les conséquences de chaque réponse.
- Ne conclus pas par préférence ; conclus par une règle de décision et des
  seuils.
- Signale explicitement les affirmations que tu n'as pas pu vérifier.
- Réponds en français.
```

## Relance possible (second passage)

```text
Attaque maintenant ta propre conclusion. Pour chaque recommandation :
1. quel fait, s'il était faux, l'invaliderait ?
2. quelle source est la plus faible ?
3. quel échec de PoC te ferait changer d'avis ?
4. quel coût caché n'apparaît pas dans ton tableau ?
Réponds en 15 lignes maximum, sans répéter la synthèse.
```
