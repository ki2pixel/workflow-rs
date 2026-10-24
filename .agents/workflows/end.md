---
description: Terminer la session et synchroniser la memory-bank
---

# `/end` — Terminer la session et synchroniser la memory-bank

1. **Charger le contexte**
   - Lire uniquement `activeContext.md` et `progress.md`.
   - Ne pas lire `productContext.md`, `systemPatterns.md` ou `decisionLog.md`
     sauf si une décision architecturale majeure a été prise pendant la
     session.
   - Pour un point ancien, faire une recherche ciblée plutôt que charger tout
     le fichier.

2. **Appliquer le protocole memory-bank**
   - Suspendre la tâche en cours, puis résumer la session.
   - Utiliser `grep`/`rg` pour identifier les fichiers et docs liés.
   - Vérifier que les chemins sont absolus
     (`/home/kidpixel/workflow-rs/memory-bank/...`).

3. **Mettre à jour la memory-bank**
   - Mettre à jour `progress.md` et `activeContext.md` en priorité.
   - Ajouter une entrée datée dans `decisionLog.md` si une décision durable a
     été prise.
   - Mettre à jour `systemPatterns.md` si un nouveau patron a été introduit.
   - Ne résumer que les deltas de la session, pas tout l'historique.

4. **Clôturer**
   - Résumer les tâches finalisées dans la réponse utilisateur.
   - Vérifier que `progress.md` indique « Aucune tâche active » si c'est le cas.
   - Vérifier que `activeContext.md` est revenu à un état neutre.
   - Vérifier qu'aucun secret ni chemin de credentials n'a été écrit.
