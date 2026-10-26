# MVP — suspension du polling quand la WebView est masquée

| Champ | Valeur |
| --- | --- |
| Type | Note |
| Statut | Référence |
| Date | 2026-10-25 |
| Portée | Suspension opt-in des pollers quand la WebView est masquée. |
| Voir aussi | `docs/contracts/CONTRAT-ENVIRONNEMENT.md`, `docs/desktop/PACKAGING-GUI.md` |

## Contexte

Le soak J4 a mesuré une croissance de `WebKitNetworkProcess` de +22,06 Mio
sur 2 h, alors que l'hôte et le `StatusWatcher` restaient plats. La cause est
le frontend legacy : `main.js` interroge `/api/system_monitor` toutes les 5 s
et `/api/csv_downloads_status` toutes les secondes, même quand la fenêtre est
minimisée dans le tray. `PollingManager` recevait bien `visibilitychange`
mais se contentait de journaliser « polling continues in background ».

## Correctif

Le correctif principal touche deux fichiers de la copie Rust du frontend :

- `assets/web/static/utils/PollingManager.js` ;
- `assets/web/static/main.js`.

Sept modules important `PollingManager.js` voient en plus leur import
versionné (`?v=20261025a`) pour contourner le cache de modules WebKit.

### `PollingManager`

Nouvelle option opt-in `pauseWhenHidden` sur `startPolling(name, callback,
interval, options)` :

```js
pollingManager.startPolling('systemMonitor', pollSystemMonitor, 5000, {
    immediate: true,
    pauseWhenHidden: true
});
```

Comportement :

- à `visibilitychange` avec `document.hidden === true`, les intervalles
  opt-in sont annulés et déplacés dans `pausedPollers` ;
- un backoff en cours (`pendingResumes`) est également suspendu : il ne
  réveille pas le poller pendant le masquage ;
- au retour à `document.hidden === false`, l'intervalle est recréé et un
  rafraîchissement immédiat est déclenché pour éviter un état périmé ;
- `startPolling` appelé alors que la page est déjà masquée diffère
  l'exécution immédiate et l'intervalle ;
- `stopPolling` et `destroy` nettoient aussi les pollers suspendus ;
- `getStats()` expose `paused` et `totalPaused` pour les diagnostics.

Les autres pollers (étapes, séquence, timers, performance) ne sont pas
opt-in : ils continuent de tourner pour ne pas bloquer l'auto-mode ou la
progression d'une séquence pendant que la fenêtre est dans le tray.

### `main.js`

Les deux pollers répétitifs passent `pauseWhenHidden: true` :

- `localDownloadsStatus` (`/api/csv_downloads_status`) ;
- `systemMonitor` (`/api/system_monitor`).

Aucun ID, contrat JSON, ordre de clés ou route serveur n'est modifié.

## Validation

### Vérification live

Serveur de test `:5005` avec `RUST_LOG=workflow_server=debug,tower_http=debug`,
client Tauri issu du `.deb` J5, cache WebKit isolé, fenêtre minimisée avec
`xdotool` puis restaurée avec `wmctrl`.

Le script `scripts/poc/verify-visibility-polling.py` automatise la mesure :

```sh
python3 scripts/poc/verify-visibility-polling.py \
  --server-log /tmp/opencode/mvp-frontend/server.log \
  --hidden-seconds 8 --visible-seconds 6
```

Résultat du 2026-10-25 (imports versionnés actifs) :

```json
{
  "hidden": {
    "/api/system_monitor": 0,
    "/api/csv_downloads_status": 0
  },
  "visible": {
    "/api/system_monitor": 2,
    "/api/csv_downloads_status": 6
  }
}
```

Les compteurs sont des requêtes HTTP (`path=...`), pas des lignes de trace
dupliquées. `document.hidden` et `document.visibilityState` ont été vérifiés
par un probe temporaire : `blur` et `visibilitychange` passent bien à
`hidden=true` sous WebKitGTK 2.50.4, puis à `visible` au focus.

### Endurance 10 min

Soak `mvp-frontend-soak10-20261025` avec la fenêtre masquée pendant toute la
durée, GUI Tauri et serveur `:5005`, mesure `scripts/poc/j4-monitor.py` :

| Métrique | Valeur |
|---|---:|
| Durée / échantillons | 599,236 s / 600 |
| Fenêtre masquée | 100 % (0 violation) |
| Dérive tick p95 / max | 0,323 ms / 1,953 ms |
| Hôte (RSS) | +4 Kio |
| GUI (RSS / PSS) | +88 Kio / +73 Kio |
| `WebKitNetworkProcess` (RSS) | **0 Kio** |
| `WebKitWebProcess` (RSS) | +84 Kio |
| Serveur (RSS) | 0 Kio |
| VRAM pic / libre min | 368 Mio / 3 532 Mio |
| Watcher | 602 ticks, 0 erreur, 0 skipped |
| Tray | `Idle`, 0 changement |
| Acceptation | `true` (tous les gates) |

À comparer au soak J4 de 2 h : `WebKitNetworkProcess` +22,06 Mio et PSS GUI
+25,46 Mio. Sur 10 min masquées, le réseau est plat et l'hôte reste sous
0,1 Mio de dérive. Le soak complet de 2 h reste à rejouer avant la bascule
MVP, mais la cause identifiée en J4 est neutralisée.

## Cache WebKit et déploiement

`index_new.html` versionne `main.js` avec `?v={{ cache_buster }}`, mais les
imports ES internes (`./utils/PollingManager.js`, etc.) ne portent pas ce
paramètre. Sans en-tête de cache, WebKit peut donc conserver un ancien
`PollingManager.js` après une mise à jour du serveur : c'est ce qui a été
observé lors du premier essai, avant isolation du cache WebKit.

Le serveur ajoute `Cache-Control: no-store, no-cache, must-revalidate,
max-age=0` et `Pragma: no-cache` aux réponses `/static/`. Surtout, l'import
de `PollingManager.js` est versionné dans les sept modules qui le
consomment (`./utils/PollingManager.js?v=20261025a`). Changer la clé d'URL
est indispensable : un premier essai avec `no-cache` seul a montré que
WebKit réutilisait encore le module après un HEAD 200, malgré l'en-tête.
Toute modification future de `PollingManager.js` doit incrémenter ce
suffixe (`?v=...`) dans les imports. Test associé :
`given_static_asset_when_served_then_cache_control_forces_revalidation`.

## Limites

- La suspension repose sur `document.hidden` ; un futur toolkit qui ne
  propagerait pas la visibilité au moment de la minimisation nécessiterait
  un signal hôte explicite.
- Les requêtes déjà en vol au moment du masquage ne sont pas annulées ; elles
  se terminent, puis l'intervalle est supprimé.
- Le frontend Python legacy n'est pas modifié : le correctif est une
  divergence assumée de la copie Rust, documentée dans
  `memory-bank/systemPatterns.md`.
- Le single-instance reste différé au MVP ; deux GUI ouverts créent deux
  pollers et deux sessions.

## Fichiers

- `assets/web/static/utils/PollingManager.js`
- `assets/web/static/main.js`
- imports versionnés dans `apiService.js`, `uiUpdater.js`, `timerManager.js`,
  `sequenceManager.js`, `utils/PerformanceMonitor.js` et
  `utils/PerformanceOptimizer.js` (`?v=20261025a`)
- `crates/workflow-server/src/http.rs` (`no-store` sur `/static/`)
- `scripts/poc/verify-visibility-polling.py`
- `memory-bank/systemPatterns.md`
