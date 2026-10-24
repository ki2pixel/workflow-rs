---
name: documentation
description: Rédaction technique et README pour workflow-rs : TL;DR, problème d'abord, trade-offs, style naturel.
---

# Documentation et rédaction technique

## Structure d'un article technique

### 1. TL;DR d'abord

Commencer par 1–2 phrases qu'un lecteur pressé peut parcourir. Mettre
l'insight en gras.

```markdown
**TL;DR** : Le contrat JSON de STEP6 est un index par frame, pas une copie du
tracking. Toute transformation doit préserver cet index et l'ordre des clés.
```

### 2. Commencer par le problème

Plonger le lecteur dans un scénario concret. Ne pas commencer par une
définition.

```markdown
❌ « Le pré-traitement AE est l'étape qui transforme les données... »

✅ « Le JSX reçoit un tracking de 300 Mo et doit afficher un segment en moins
de 100 ms. Sans index, il charge tout le fichier à chaque frame. »
```

### 3. Nommer les concepts

Donner un nom stable aux idées récurrentes : « contrat d'environnement »,
« réduction streaming », « manifeste STEP7 », « couche de parité ».

### 4. Utiliser des blocs ❌/✅

Montrer le mauvais réflexe et la bonne approche.

### 5. Adresser les malentendus

Expliciter ce que le lecteur pourrait croire :

- « La parité n'est pas l'identité binaire ; c'est l'identité sémantique à
  `1e-6`. »
- « Le mode CPU n'est pas un fallback : c'est un mode de test. »

### 6. Utiliser une analogie cohérente

Une seule analogie par document, filée jusqu'au bout.

### 7. Inclure un tableau de trade-offs

| Option | Avantages | Inconvénients | Décision |
| --- | --- | --- | --- |
| Python partagé | Zéro duplication | Couplage, deux stacks | Rejeté |
| Runtime Rust isolé | Frontière nette | Sync explicite | Retenu |

### 8. Terminer par une règle d'or

```markdown
**Règle d'or** : le contrat JSON est la source de vérité ; tout le reste
(JSX, CEP, STEP8) s'y adapte, pas l'inverse.
```

### 9. Exemples concrets

Un exemple réel de production vaut mieux que trois exemples synthétiques.

## Voix et style

- Choisir entre narratif (article) et instructionnel (guide).
- Phrases courtes, verbes actifs.
- Expliquer le « pourquoi » avant le « comment ».
- Éviter le ton académique ou corporate.
- Ne pas utiliser de jargon non défini.
- Ne pas noyer le lecteur sous les précautions.

## Éviter le style « IA »

### Signaux

- Phrases creuses (« il est important de noter que »).
- Listes sans contenu.
- Adverbes empilés.
- Conclusions qui répètent l'introduction.
- Ton promotionnel.
- Emphase excessive (`**très**`, `**crucial**`).
- Titres génériques (« Introduction », « Conclusion »).

### Écrire naturellement

- Varier la longueur des phrases.
- Utiliser des exemples concrets.
- Supprimer les phrases qui ne disent rien.
- Préférer les faits aux adjectifs.

## Structure d'un README

- **Pourquoi** : le problème résolu, pas seulement la liste des fichiers.
- **Démarrage rapide** : commandes copiables.
- **Architecture** : ce qui vit où, en une page.
- **Contrats** : entrées/sorties, formats, invariants.
- **Limites** : ce qui n'est pas couvert.
- **Liens** : docs de lot, parité, memory-bank.

### Bon exemple

```markdown
# workflow-rs

Réécriture Rust du pipeline workflow_mediapipe. Le Python reste la référence
de production et le rollback.

## Démarrage

cargo run -p workflow-cli -- config check
cargo run --release -p workflow-server -- --config config/config.toml
```

### Mauvais exemple

```markdown
# Workflow

Ce projet contient de nombreux fichiers et dossiers. Il est important de bien
lire la documentation.
```

## Ponctuation

- Éviter ` - ` comme séparateur ; préférer `;`, `:`, `—` ou une phrase.
- Une idée par paragraphe.
- Pas de double espace.
- Accents corrects dans le texte français.
- Les commandes, chemins et identifiants restent en anglais.

## Documentation Rust

- `///` pour les items publics.
- `//!` pour les modules.
- Expliquer les invariants, les unités et les erreurs.
- Ne pas paraphraser le nom de la fonction.
- Un exemple `# Examples` seulement s'il apporte quelque chose.
- Les commentaires de bloc expliquent une décision, pas la syntaxe.

## Documentation du dépôt

- `README.md` : état, commandes, prérequis.
- `docs/LOT-*.md` : périmètre, parité, performance, limites.
- `docs/CONTRAT-ENVIRONNEMENT.md` : routes, états, schémas.
- `parity/DRIFT.md` : écarts et décisions.
- `memory-bank/` : état courant, décisions, progrès.
- `.agents/` : règles, skills, workflows pour les agents.

## Processus de relecture

- [ ] Le TL;DR existe si le document est long.
- [ ] Le problème est posé avant la solution.
- [ ] Les concepts sont nommés.
- [ ] Les trade-offs sont explicites.
- [ ] Les commandes sont copiables et testées.
- [ ] Les limites sont dites.
- [ ] Le style est naturel, sans artefacts IA.
- [ ] Les liens et chemins sont valides.
