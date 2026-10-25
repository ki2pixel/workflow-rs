# Revue finale — crate `workflow-desktop-services`

Date : 2026-10-25
Périmètre : `crates/workflow-desktop-services` au commit `b2d9ffd`, complété
par les corrections de gel ci-dessous.
Méthode : lecture ligne à ligne, revue des frontières HTTP/tray/audio,
exécution des tests, confrontation aux preuves J4 (matrice 8 états, soak 2 h)
et J5 (paquet, systemd user). Aucun test matériel n'est relancé ici.

## Verdict

**Gel v0.1.0 accepté.** L'architecture est saine : le crate ne dépend d'aucun
toolkit graphique, les adaptateurs Tauri implémentent des traits, le watcher
public dans un canal `watch` à slot unique et les erreurs sont typées. Trois
corrections P2 ont été appliquées avant le gel ; les risques P3 restants sont
documentés et acceptés.

## 1. Carte d'architecture

| Module | Rôle | Dépendance graphique |
|---|---|---|
| `auth.rs` | extraction et protection du `worker-token` / CSRF | non |
| `client.rs` | client HTTP typé, validation loopback, retry 401 GET | non |
| `models.rs` | modèles JSON du contrat serveur, `StepKey` | non |
| `polling.rs` | `StatusWatcher`, snapshots, `terminal_seq`, statistiques | non |
| `tray.rs` | politique pure du tray (état, progression, menu) | non |
| `notify.rs` | notifications D-Bus et `TransitionGate` | non (D-Bus) |
| `audio.rs` | lecture MP3 locale via `rodio` | non (ALSA) |
| `logs.rs` | classification et buffer borné | non |
| `telemetry.rs` | RSS/PSS et JSONL | non |
| `error.rs` | `ServicesError` typée | non |

Flux principal :

```text
GET /  ──► auth::parse_session_html ──► DesktopClient (token + CSRF)
                                              │
                                              ▼
                         polling::StatusWatcher (1 Hz, timeouts)
                                              │ watch::Sender (1 slot)
                                              ▼
                         tray::TrayStatus::from_snapshot
                                              │
                              ┌───────────────┴───────────────┐
                              ▼                               ▼
                    TauriTray (hôte)                 notify/audio (opt-in)
```

Le crate ne connaît ni Tauri, ni GTK, ni la WebView. Les traits
`TrayPresenter`, `Notifier` et `AudioPlayer` sont les points d'inversion ;
`NoopTray`, `NoopNotifier` et `NoopAudio` servent aux tests et aux modes
désactivés.

## 2. `auth.rs`

### Invariants vérifiés

- Le token est lu dans `<meta name="worker-token">` du HTML Minijinja, jamais
  dans `config/config.toml` et jamais placé dans une URL.
- `WorkerToken` zéroïse son buffer au drop (`ZeroizeOnDrop`) et son `Debug`
  affiche `[redacted]`.
- Le token est refusé s'il est vide, fait plus de 512 octets ou contient un
  caractère de contrôle.
- `Session` redacte aussi le CSRF dans `Debug`.
- Les entités HTML (`&amp;`, `&#x27;`) sont décodées par `scraper` ; les
  tests couvrent l'ordre inverse des attributs.
- `sha256_hex` sert à l'audit A-Strict, pas à une comparaison de secret.

### Constats

| ID | Sévérité | Constat | Décision |
|---|---|---|---|
| AUTH-1 | P3 | `Session.csrf_token` est un `String` non zéroïsé au drop. | Accepté : secret de session en mémoire locale, durée de vie courte ; un wrapper `Zeroizing` est recommandé en v0.2. |
| AUTH-2 | P3 | `WorkerToken::expose()` retourne `&str` et permet un usage hors en-tête HTTP. | Accepté : API interne au workspace ; le seul appelant est `client.rs`. |

Aucun finding P1/P2. La séparation « parse HTML → token typé → header » est
correcte et testée.

## 3. `polling.rs`

### Invariants vérifiés

- **Slot unique** : `watch::channel(StatusSnapshot::initial())` ; un snapshot
  lent est écrasé, jamais mis en file. La mémoire ne dépend pas de la durée
  du soak.
- **Tick ancré** : `next += interval` ; si le poll dépasse l'intervalle,
  `skipped` est incrémenté et `next` repart de `now + interval`. Pas de
  dérive cumulative.
- **Timeout par requête** : `request_timeout` (800 ms par défaut) entoure
  `sequence_status` et `step_status`.
- **Tray increvable** : `PollingConfig::tray()` met `max_errors = None` ;
  le watcher affiche `Offline` et continue.
- **Premier poll** : `first_poll` mémorise `last_outcome` sans le notifier,
  pour qu'un GUI fraîchement lancé ne rejoue pas une séquence terminée.
- **Reconnexion** : après 3 erreurs, `was_unreachable = true` ; au retour,
  `last_step`, `last_step_key`, `last_terminal`, `previous_outcome` et
  `first_poll` sont réinitialisés pour éviter une notification fantôme.
- **`terminal_seq`** : monotone, incrémenté une seule fois par terminal
  (séquence prioritaire sur étape) ; `TransitionGate` s'en sert pour
  dédupliquer.
- **Statistiques** : `ticks`, `successes`, `errors`, `skipped`,
  `last_latency_ms`, `last_success_unix_ms`, `consecutive_errors` en
  atomiques relâchés, snapshot cohérent par lecture indépendante.

### Constats

| ID | Sévérité | Constat | Décision |
|---|---|---|---|
| POLL-1 | P2 | Si le récepteur `watch` disparaît (tray paniqué ou arrêté), `let _ = tx.send(...)` ignorait l'erreur et le watcher continuait à interroger le serveur indéfiniment. | **Corrigé avant gel** : `tx.send(...).is_err()` interrompt la boucle ; test `watcher_stops_when_receiver_is_dropped`. |
| POLL-2 | P3 | Pas de test async de la dérive ni de la reconnexion ; un test couvre désormais l'arrêt sans récepteur. | Accepté : la matrice J4 (8/8 états) et le soak 2 h (7 201 ticks, 0 erreur) sont la preuve d'intégration. Toute modification de `polling.rs` doit rejouer ce protocole. |
| POLL-3 | P3 | Un échec de `step_status` donne `connection = Degraded` avec `consecutive_errors = 0` et `error = None` ; le tray affiche « Statut d’étape indisponible » sans détail. | Accepté : comportement visuel volontairement non bloquant ; le détail serait utile en v0.2. |

Le point le plus important — l'absence de file illimitée — est structurel :
`watch` ne conserve qu'une valeur. C'est ce qui rend le soak J4 mémoire-plat
côté hôte.

## 4. `client.rs` et `error.rs`

### Invariants vérifiés

- `validate_base_url` n'accepte que `http://` et les hôtes loopback
  (`127.0.0.1`, `::1`).
- Le token et le CSRF sont ajoutés en en-têtes, jamais en query string.
- Un 401 sur GET déclenche un `bootstrap_session` puis un seul retry ; les
  POST ne sont pas rejoués (pas d'effet de bord dupliqué).
- Les endpoints sont typés (`run_step`, `stop_step`, `cancel_step`,
  `step_status`, `sequence_status`, `system_monitor`, `specific_log`,
  `fetch_sound`).
- `fetch_sound` refuse les noms vides, `..`, `/` et `\`.
- Les erreurs sont typées (`Url`, `Http`, `HttpStatus`, `AuthExpired`,
  `Token`, `NotRustServer`, `Json`, `Config`, `Notify`, `Io`) et
  `#[non_exhaustive]` autorise l'ajout d'une variante sans casser les
  adaptateurs.

### Constats

| ID | Sévérité | Constat | Décision |
|---|---|---|---|
| CLI-1 | P2 | `validate_base_url` acceptait `http://user:pass@127.0.0.1:5005/` ; `reqwest` aurait pu envoyer des identifiants Basic. | **Corrigé avant gel** : username/password non vides refusés + test. |
| CLI-2 | P3 | `HttpStatus.body` n'est pas borné et peut contenir une page HTML ou un JSON volumineux ; `Display` l'inclut. | Accepté : serveur loopback, body utile au diagnostic ; troncature (2 Kio) recommandée en v0.2. |
| CLI-3 | P3 | Le retry 401 est réservé aux GET ; un POST expiré remonte `HttpStatus(401)`. | Accepté et documenté : éviter un doublon de lancement/arrêt. |

## 5. Autres modules

- **`models.rs`** : `StepKey` est dupliqué volontairement pour éviter une
  dépendance à `workflow-server`. Les tests couvrent le round-trip, les logs
  optionnels et les variantes GPU. Risque P3 accepté : dérive possible avec
  `catalog.rs`, à vérifier lors de tout ajout d'étape.
- **`tray.rs`** : politique pure, `visual_key` ignore le timestamp pour
  éviter les mises à jour inutiles, progression fractionnaire prioritaire
  avec fallback entier et clamp 0–100. **Correction P2 appliquée** : une
  séquence active prime désormais sur `last_terminal`, pour ne pas afficher
  un faux « Succès » entre deux étapes.
- **`notify.rs`** : `TransitionGate` déduplique par `terminal_seq`, y compris
  deux terminaux identiques successifs ; testé.
- **`audio.rs`** : noms de fichiers fixes, erreur de fichier avant ouverture
  du device, lecture dans `spawn_blocking` côté hôte.
- **`logs.rs`** : buffer `VecDeque` borné, classification par regex
  précompilées, `html_unescape` sans double décodage.
- **`telemetry.rs`** : lecture `/proc/self`, écriture JSONL append ;
  pas de rotation (module non utilisé par l'hôte Tauri aujourd'hui).

## 6. Corrections appliquées avant gel

| Correction | Test |
|---|---|
| `tray.rs` : séquence active avant terminal obsolète | `running_sequence_ignores_stale_terminal_between_steps` |
| `polling.rs` : arrêt si le récepteur `watch` disparaît | `watcher_stops_when_receiver_is_dropped` |
| `client.rs` : refus des identifiants dans l'URL | `accepts_loopback_http_only` étendu |
| `error.rs` : `#[non_exhaustive]` | compilation workspace |

Commandes exécutées :

```sh
cargo fmt --all --check
cargo clippy -p workflow-desktop-services --all-targets -- -D warnings
cargo test -p workflow-desktop-services          # 38 tests
cargo test -p workflow-server --lib              # 43 tests
cargo test --workspace                           # 319 tests
cargo clippy --workspace --all-targets -- -D warnings
```

## 7. Contrat de gel v0.1.0

**Stable :**

- signatures publiques de `auth`, `client`, `polling`, `tray`, `notify`,
  `audio`, `logs`, `telemetry` ;
- sémantique du canal `watch` à slot unique et de `terminal_seq` ;
- format des modèles JSON (ordre des clés côté serveur inchangé) ;
- invariants A-Strict : loopback HTTP, token hors URL, zéroïzation du token,
  aucun toolkit graphique dans le crate.

**Autorisé sans casser le gel :**

- ajout d'un module ou d'un adaptateur (ex. `GtkTray`) ;
- ajout d'une variante à `ServicesError` (grâce à `#[non_exhaustive]`) ;
- correction de bug accompagnée d'un test et d'une note de revue ;
- ajout de champs optionnels aux modèles si le serveur les sérialise déjà.

**Interdit sans nouvelle revue majeure :**

- changer les signatures de `StatusWatcher`, `DesktopClient` ou des traits ;
- modifier la sémantique de `terminal_seq` ou de `TransitionGate` ;
- déplacer la lecture du token hors de `auth.rs` ;
- réintroduire une file non bornée ou un `setInterval` hors `PollingManager` ;
- modifier `assets/web` sans documenter la divergence.

Toute modification de `polling.rs` ou `tray.rs` doit rejouer la matrice J4
(8 états) et, pour un changement de charge, le soak 2 h.

## 8. Preuves et risques résiduels

- J4 : matrice `j4-matrix-final-20261025-050025`, 8/8 états, tray D-Bus
  stable, restauration par `com.canonical.dbusmenu.Event`.
- J4 soak : 7 200 échantillons, 7 199,265 s, dérive p95 0,310 ms, hôte
  +0,03 Mio, watcher 7 201 ticks / 0 erreur / 0 skipped, 0 OOM.
- J5 : `.deb` 5,59 Mio, extraction sans root, systemd user PPID 1644,
  GUI jamais parent, `systemd-analyze verify` rc=0.
- Risques résiduels : couverture async du watcher, duplication `StepKey`,
  body d'erreur non borné, CSRF non zéroïsé, single-instance différé.
- La dérive `WebKitNetworkProcess` est traitée côté frontend par
  `pauseWhenHidden` (voir `docs/MVP-FRONTEND-POLLING.md`) ; le crate partagé
  n'est pas modifié pour ce point.

## 9. Conclusion

Le crate peut être figé comme socle commun du client Tauri et d'un futur
repli GTK4. Les trois corrections P2 ne changent pas les contrats HTTP/JSON ;
elles durcissent l'arrêt du watcher, la priorité d'état du tray et la
validation d'URL. Le gel est prononcé pour `workflow-desktop-services
0.1.0`, avec obligation de rejouer J4 pour toute évolution de `polling.rs`
ou `tray.rs`.
