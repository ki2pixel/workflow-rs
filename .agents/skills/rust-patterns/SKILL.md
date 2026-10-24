---
name: rust-patterns
description: Modèles idiomatiques de Rust, ownership, gestion des erreurs, traits, concurrence et bonnes pratiques pour construire des applications sûres et efficaces.
---

# Modèles de développement Rust

Modèles idiomatiques et bonnes pratiques de Rust pour construire des applications sûres, efficaces et maintenables.

## Quand l'utiliser

- Écrire du nouveau code Rust
- Réviser du code Rust
- Refactoriser du code Rust existant
- Concevoir la structure des crates et l'organisation des modules

## Comment ça fonctionne

Ce skill renforce les conventions idiomatiques de Rust dans six domaines clés : l'ownership et le borrowing pour prévenir les courses de données à la compilation, la propagation des erreurs avec `Result`/`?` en utilisant `thiserror` pour les bibliothèques et `anyhow` pour les applications, les enums et le pattern matching exhaustif pour rendre les états invalides impossibles, les traits et les génériques pour des abstractions à coût nul, la concurrence sûre avec `Arc<Mutex<T>>`, les canaux et async/await, ainsi que des surfaces `pub` minimales organisées par domaine.

> Cette skill complète `.agents/rules/codingstandards.md` et
> `rust-concurrency-orchestrator` ; en cas de conflit, les règles du dépôt
> priment. Dans workflow-rs, les tests sont inline sous `#[cfg(test)]` et il
> n'y a pas de dossier `tests/`.

## Principes fondamentaux

### Ownership et borrowing

Le système d'ownership de Rust prévient les courses de données et les erreurs mémoire à la compilation.

```rust
// Bien : Passer des références quand l'ownership n'est pas nécessaire
fn process(data: &[u8]) -> usize {
    data.len()
}

// Bien : Prendre l'ownership uniquement lorsqu'il faut stocker ou consommer
fn store(data: Vec<u8>) -> Record {
    Record { payload: data }
}

// Mauvais : Cloner inutilement pour contourner le borrow checker
fn process_bad(data: &Vec<u8>) -> usize {
    let cloned = data.clone(); // Coûteux — il suffit d'emprunter
    cloned.len()
}
```

### Utiliser `Cow` pour un ownership flexible

```rust
use std::borrow::Cow;

fn normalize(input: &str) -> Cow<'_, str> {
    if input.contains(' ') {
        Cow::Owned(input.replace(' ', "_"))
    } else {
        Cow::Borrowed(input) // Coût nul lorsqu'aucune mutation n'est nécessaire
    }
}
```

## Gestion des erreurs

### Utiliser `Result` et `?` — jamais `unwrap()` en production

```rust
// Bien : Propager les erreurs avec du contexte
use anyhow::{Context, Result};

fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config from {path}"))?;
    let config: Config = toml::from_str(&content)
        .with_context(|| format!("failed to parse config from {path}"))?;
    Ok(config)
}

// Mauvais : Provoque un panic en cas d'erreur
fn load_config_bad(path: &str) -> Config {
    let content = std::fs::read_to_string(path).unwrap(); // Panic !
    toml::from_str(&content).unwrap()
}
```

### Erreurs de bibliothèque avec `thiserror`, erreurs d'application avec `anyhow`

```rust
// Code de bibliothèque : erreurs structurées et typées
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("record not found: {id}")]
    NotFound { id: String },
    #[error("connection failed")]
    Connection(#[from] std::io::Error),
    #[error("invalid data: {0}")]
    InvalidData(String),
}

// Code d'application : gestion des erreurs flexible
use anyhow::{bail, Result};

fn run() -> Result<()> {
    let config = load_config("app.toml")?;
    if config.workers == 0 {
        bail!("worker count must be > 0");
    }
    Ok(())
}
```

### Combinateurs d'`Option` plutôt que code impératif

```rust
// Bien : Chaîne de combinateurs
fn find_user_email(users: &[User], id: u64) -> Option<String> {
    users.iter()
        .find(|u| u.id == id)
        .map(|u| u.email.clone())
}

// Mauvais : Code impératif plus verbeux
fn find_user_email_bad(users: &[User], id: u64) -> Option<String> {
    for user in users {
        if user.id == id {
            return Some(user.email.clone());
        }
    }
    None
}
```

## Enums et pattern matching

### Modéliser les états avec des enums

```rust
// Bien : Les états impossibles sont irreprésentables
enum ConnectionState {
    Disconnected,
    Connecting { attempt: u32 },
    Connected { session_id: String },
    Failed { reason: String, retries: u32 },
}

fn handle(state: &ConnectionState) {
    match state {
        ConnectionState::Disconnected => connect(),
        ConnectionState::Connecting { attempt } if *attempt > 3 => abort(),
        ConnectionState::Connecting { .. } => wait(),
        ConnectionState::Connected { session_id } => use_session(session_id),
        ConnectionState::Failed { retries, .. } if *retries < 5 => retry(),
        ConnectionState::Failed { reason, .. } => log_failure(reason),
    }
}
```

### Matching exhaustif — pas de joker dans la logique métier

```rust
// Bien : Gérer chaque variante explicitement
match command {
    Command::Start => start_service(),
    Command::Stop => stop_service(),
    Command::Restart => restart_service(),
    // L'ajout d'une nouvelle variante force sa prise en charge ici
}

// Mauvais : Le joker masque les nouvelles variantes
match command {
    Command::Start => start_service(),
    _ => {} // Ignore silencieusement Stop, Restart et les variantes futures
}
```

## Traits et génériques

### Accepter des génériques, retourner des types concrets

```rust
// Bien : Entrée générique, sortie concrète
fn read_all(reader: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    Ok(buf)
}

// Bien : Bounds de traits pour des contraintes multiples
fn process<T: Display + Send + 'static>(item: T) -> String {
    format!("processed: {item}")
}
```

### Trait objects pour le dispatch dynamique

```rust
// Utiliser lorsque des collections hétérogènes ou des systèmes de plugins sont nécessaires
trait Handler: Send + Sync {
    fn handle(&self, request: &Request) -> Response;
}

struct Router {
    handlers: Vec<Box<dyn Handler>>,
}

// Utiliser des génériques lorsque la performance est requise (monomorphisation)
fn fast_process<H: Handler>(handler: &H, request: &Request) -> Response {
    handler.handle(request)
}
```

### Modèle newtype pour la sécurité des types

```rust
// Bien : Des types distincts empêchent de mélanger les arguments
struct UserId(u64);
struct OrderId(u64);

fn get_order(user: UserId, order: OrderId) -> Result<Order> {
    // Impossible d'intervertir accidentellement user ID et order ID
    todo!()
}

// Mauvais : Facile d'intervertir les arguments
fn get_order_bad(user_id: u64, order_id: u64) -> Result<Order> {
    todo!()
}
```

## Structs et modélisation des données

### Modèle builder pour une construction complexe

```rust
struct ServerConfig {
    host: String,
    port: u16,
    max_connections: usize,
}

impl ServerConfig {
    fn builder(host: impl Into<String>, port: u16) -> ServerConfigBuilder {
        ServerConfigBuilder { host: host.into(), port, max_connections: 100 }
    }
}

struct ServerConfigBuilder { host: String, port: u16, max_connections: usize }

impl ServerConfigBuilder {
    fn max_connections(mut self, n: usize) -> Self { self.max_connections = n; self }
    fn build(self) -> ServerConfig {
        ServerConfig { host: self.host, port: self.port, max_connections: self.max_connections }
    }
}

// Utilisation : ServerConfig::builder("localhost", 8080).max_connections(200).build()
```

## Itérateurs et closures

### Préférer les chaînes d'itérateurs aux boucles manuelles

```rust
// Bien : Déclaratif, lazy, composable
let active_emails: Vec<String> = users.iter()
    .filter(|u| u.is_active)
    .map(|u| u.email.clone())
    .collect();

// Mauvais : Accumulation impérative
let mut active_emails = Vec::new();
for user in &users {
    if user.is_active {
        active_emails.push(user.email.clone());
    }
}
```

### Utiliser `collect()` avec annotation de type

```rust
// Collecter dans différents types
let names: Vec<_> = items.iter().map(|i| &i.name).collect();
let lookup: HashMap<_, _> = items.iter().map(|i| (i.id, i)).collect();
let combined: String = parts.iter().copied().collect();

// Collecter des Results — court-circuite à la première erreur
let parsed: Result<Vec<i32>, _> = strings.iter().map(|s| s.parse()).collect();
```

## Concurrence

### `Arc<Mutex<T>>` pour un état mutable partagé

```rust
use std::sync::{Arc, Mutex};

let counter = Arc::new(Mutex::new(0));
let handles: Vec<_> = (0..10).map(|_| {
    let counter = Arc::clone(&counter);
    std::thread::spawn(move || {
        let mut num = counter.lock().expect("mutex poisoned");
        *num += 1;
    })
}).collect();

for handle in handles {
    handle.join().expect("worker thread panicked");
}
```

### Canaux pour le passage de messages

```rust
use std::sync::mpsc;

let (tx, rx) = mpsc::sync_channel(16); // Canal borné avec backpressure

for i in 0..5 {
    let tx = tx.clone();
    std::thread::spawn(move || {
        tx.send(format!("message {i}")).expect("receiver disconnected");
    });
}
drop(tx); // Fermer le sender pour que l'itérateur rx se termine

for msg in rx {
    println!("{msg}");
}
```

### Async avec Tokio

```rust
use anyhow::{Context, Result};
use tokio::time::Duration;

async fn fetch_with_timeout(url: &str) -> Result<String> {
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        reqwest::get(url),
    )
    .await
    .context("request timed out")?
    .context("request failed")?;

    response.text().await.context("failed to read body")
}

// Lancer des tâches concurrentes
async fn fetch_all(urls: Vec<String>) -> Result<Vec<Result<String>>> {
    let handles: Vec<_> = urls
        .into_iter()
        .map(|url| tokio::spawn(async move { fetch_with_timeout(&url).await }))
        .collect();

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        results.push(match handle.await {
            Ok(result) => result,
            Err(error) => Err(anyhow::anyhow!("spawned task failed: {error}")),
        });
    }
    Ok(results)
}
```

## Code unsafe

### Quand `unsafe` est acceptable

```rust
// Acceptable : frontière FFI avec invariants documentés
/// # Safety
/// `ptr` must be a valid, aligned pointer to an initialized `Widget`
/// that remains valid for `'a`.
unsafe fn widget_from_raw<'a>(ptr: *const Widget) -> &'a Widget {
    // SAFETY: l'appelant garantit que ptr est valide et aligné
    unsafe { &*ptr }
}

// Acceptable : Chemin critique de performance avec preuve de correction
// SAFETY: index is always < len due to the loop bound
unsafe { slice.get_unchecked(index) }
```

### Quand `unsafe` n'est pas acceptable

```rust
// Mauvais : Utiliser unsafe pour contourner le borrow checker
// Mauvais : Utiliser unsafe par commodité
// Mauvais : Utiliser unsafe sans commentaire Safety
// Mauvais : Faire un transmute entre des types non liés
```

## Système de modules et structure des crates

### Organiser par domaine, non par type

```text
my_app/
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── auth/          # Module de domaine
│   │   ├── mod.rs
│   │   ├── token.rs
│   │   └── middleware.rs
│   ├── orders/        # Module de domaine
│   │   ├── mod.rs
│   │   ├── model.rs
│   │   └── service.rs
│   └── db/            # Infrastructure
│       ├── mod.rs
│       └── pool.rs
├── benches/           # Benchmarks
└── Cargo.toml
```

Dans workflow-rs, les tests restent inline sous `#[cfg(test)]` dans chaque
module ; il n'y a pas de dossier `tests/` d'intégration.

### Visibilité — exposer le minimum

```rust
// Bien : pub(crate) pour partager en interne
pub(crate) fn validate_input(input: &str) -> bool {
    !input.is_empty()
}

// Bien : Ré-exporter l'API publique depuis lib.rs
pub mod auth;
pub use auth::AuthMiddleware;

// Mauvais : Tout mettre en pub
pub fn internal_helper() {} // Devrait être pub(crate) ou privé
```

## Intégration avec les outils

### Commandes essentielles

```bash
# Compiler et vérifier
cargo build --workspace
cargo check --workspace      # Vérification rapide des types sans codegen
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check

# Tests
cargo test --workspace
cargo test -p workflow-steps step6 --lib
cargo test --workspace -- --nocapture

# Dépendances
cargo audit              # Audit de sécurité
cargo tree               # Arbre des dépendances
cargo update             # Mettre à jour les dépendances

# Performance
cargo bench              # Exécuter les benchmarks
```

## Référence rapide : idiomes Rust

| Idiome | Description |
| --- | --- |
| Emprunter, ne pas cloner | Passer `&T` au lieu de cloner à moins que l'ownership ne soit nécessaire |
| Rendre les états illégaux irreprésentables | Utiliser des enums pour modéliser uniquement les états valides |
| `?` au lieu de `unwrap()` | Propager les erreurs, ne jamais provoquer de panic en bibliothèque/production |
| Parser, ne pas valider | Convertir les données non structurées en structs typées à la frontière |
| Newtype pour la sécurité des types | Envelopper les primitifs dans des newtypes pour prévenir l'inversion d'arguments |
| Préférer les itérateurs aux boucles | Les chaînes déclaratives sont plus claires et souvent plus rapides |
| `#[must_use]` sur les Results | S'assurer que les appelants gèrent les valeurs de retour |
| `Cow` pour un ownership flexible | Éviter les allocations lorsque le borrowing suffit |
| Matching exhaustif | Pas de joker `_` pour les enums critiques du domaine métier |
| Surface `pub` minimale | Utiliser `pub(crate)` pour les API internes |

## Anti-modèles à éviter

```rust
// Mauvais : .unwrap() dans du code de production
let value = map.get("key").unwrap();

// Mauvais : .clone() pour satisfaire le borrow checker sans comprendre pourquoi
let data = expensive_data.clone();
process(&original, &data);

// Mauvais : Utiliser String quand &str suffit
fn greet(name: String) { /* devrait être &str */ }

// Mauvais : Box<dyn Error> dans des bibliothèques (utiliser thiserror à la place)
fn parse(input: &str) -> Result<Data, Box<dyn std::error::Error>> { todo!() }

// Mauvais : Ignorer les avertissements must_use
let _ = validate(input); // Ignore silencieusement un Result

// Mauvais : Bloquer dans un contexte async
async fn bad_async() {
    std::thread::sleep(Duration::from_secs(1)); // Bloque l'executor !
    // Utiliser : tokio::time::sleep(Duration::from_secs(1)).await;
}
```

**À retenir** : le compilateur élimine les courses de données et de nombreux
états invalides, mais il ne remplace ni les tests, ni la revue, ni la validation
de parité.

## Références

- `.agents/rules/codingstandards.md`
- `.agents/rules/test-strategy.md`
- `.agents/skills/rust-concurrency-orchestrator/SKILL.md`
