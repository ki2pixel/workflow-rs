---
trigger: always_on
description: Protocole de la memory-bank de workflow-rs (lecture chirurgicale, mise à jour, rétention)
globs:
---

# Protocole memory-bank — workflow-rs

## Vue d'ensemble

La memory-bank conserve l'état courant du projet, les décisions et l'historique
récent. Elle évite de relire tout le dépôt à chaque session. Le dossier est
versionné et vit à la racine du dépôt.

**Règle de base : lecture chirurgicale, mise à jour en fin de tâche.**

## 1. Initialisation sélective

### Déclencheur : première interaction | Priorité : immédiate | Requis : vrai

1. **Lecture MCP** : appeler
   `fast_read_file(path="/home/kidpixel/workflow-rs/memory-bank/activeContext.md")`.
2. **Intégration** : vérifier les blocages, la tâche en cours et les prochaines
   actions.
3. **Contrainte stricte** : ne pas charger `productContext.md` ni
   `systemPatterns.md` sauf besoin architectural ou stratégique explicite.
4. **Préfixe** : commencer la réponse par `[MEMORY BANK: ACTIVE (MCP-PULL)]`.
5. **Tolérance aux pannes** : si `fast_read_file` échoue ou renvoie le
   contenu d'un autre projet (racine MCP hors dépôt), le signaler et utiliser
   l'outil de lecture standard sur le chemin absolu.
6. **Interdiction** : ne jamais charger plus d'un fichier à la fois.
7. **Chemins absolus** : toujours utiliser
   `/home/kidpixel/workflow-rs/memory-bank/...`.
8. **Racine MCP** : le serveur `fast-filesystem` est enraciné hors du dépôt
   (`/home/kidpixel/kimi-proxy`). Un chemin relatif `memory-bank/progress.md`
   lit donc le mauvais projet. Vérifier que le contenu renvoyé appartient bien à
   `workflow-rs` avant toute écriture.
9. **Fallback d'écriture** : si `fast_write_file` refuse un chemin hors racine,
   utiliser l'outil d'écriture standard de l'agent après avoir confirmé le
   chemin absolu. Ne jamais écrire dans le memory-bank d'un autre projet.

## 2. Structure et responsabilités

Cinq fichiers, pas plus, à la racine de `memory-bank/` :

- **`productContext.md`** : périmètre, objectifs, architecture, pipeline,
  intégrations et non-objectifs.
- **`activeContext.md`** : état de la session, décisions actives, blocages,
  prochaines actions.
- **`systemPatterns.md`** : patrons récurrents (Rust, config, JSON, serveur,
  TPU, tests, docs, frontend, sync).
- **`decisionLog.md`** : décisions techniques, alternatives rejetées, renvois
  vers `parity/DRIFT.md` et `docs/lots/LOT-*.md`.
- **`progress.md`** : état d'avancement (terminé, en cours, prochain, problèmes).

Ne pas créer de nouveaux types de fichiers à la racine sans autorisation.

## 3. Mise à jour et qualité

### Protocole de mise à jour

- **Fréquence** : à la fin d'une tâche ou sur commande `UMB`.
- **Horodatage** : `[YYYY-MM-DD HH:MM:SS] - [Résumé]` pour chaque entrée.
- **Concision** : entrées focalisées et actionnables.
- **Références croisées** : lier les entrées entre fichiers et vers les docs de
  lot / décisions de parité.

### Rétention

- **90 jours de détail** dans `decisionLog.md` et `progress.md`.
- **Archivage** : résumer les entrées plus anciennes et les déplacer vers
  `memory-bank/archives/*.md` — uniquement quand le volume le justifie.
- **Traçabilité** : mentionner le chemin d'archive dans le fichier actif.
- **Intégrité** : avant de réécrire un fichier, vérifier qu'il ne contient pas
  de marqueur `⟦PRUNÉ…⟧` laissé par le pruner MCP ; dans ce cas, relire la
  source brute.

## 4. Règles contextuelles

### A. Contexte documentation

- **Déclencheur** : question sur `docs`, `README`, `guide`, `référence API`.
- **Instruction** : annoncer « Je consulte la documentation interne du
  projet. » avant de répondre.
- **Priorité** : `README.md`, `AGENTS.md`, `docs/architecture/CONTRAT-ENVIRONNEMENT.md`,
  `docs/lots/LOT-*.md`, `parity/DRIFT.md`.
- **Conflit** : si le code et la doc divergent, citer les deux et demander
  arbitrage.

### B. Contexte code et architecture

- **Déclencheur** : génération, modification, refactor, question
  d'architecture.
- **Instruction** : annoncer « J'applique les standards obligatoires du
  projet. »
- **Lecture** : `.agents/rules/codingstandards.md` immédiatement.
- **Contrainte** : formuler le plan strictement à partir de ces standards.

## 5. Commande spéciale : Update Memory Bank (UMB)

- **Déclencheur** : `^(Update Memory Bank|UMB)$`.
- **Processus** :
  1. **Arrêt** : suspendre l'activité en cours.
  2. **Accusé** : répondre `[MEMORY BANK: UPDATING]`.
  3. **Audit** : relire la session pour les décisions, changements et
     clarifications.
  4. **Sync** : mettre à jour `progress.md` et `activeContext.md` en priorité,
     puis `decisionLog.md` si une décision durable a été prise.
  5. **Nettoyage** : ne résumer que les deltas de la session, pas tout
     l'historique.

## 6. Observabilité

Annoncer explicitement l'intention lors des lectures :

- « Validation pré-vol (lecture de activeContext)... »
- « Lecture des patrons d'architecture pour une tâche de code... »
- « Synchronisation de la memory-bank (mode UMB)... »

## 7. Ce que la memory-bank n'est pas

- Un doublon de `README.md` ou des `docs/lots/LOT-*.md`.
- Un journal exhaustif de chaque commit.
- Un espace de brouillon pour les expérimentations.
- Une source de vérité pour les secrets ou les données de production.
