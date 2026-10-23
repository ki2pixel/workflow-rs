# Bascule publique par reverse proxy

| Champ | Valeur |
| --- | --- |
| Date | 2026-10-23 |
| Objet | Router le trafic public vers Rust 5004, avec Python 5003 en rollback |
| Principe | Un seul backend actif pour les routes workflow, jamais de répartition par requête |
| Prérequis | Les deux serveurs restent liés à `127.0.0.1` ; seul le proxy écoute sur 80/443 |

## 0. Situation actuelle

Aujourd'hui l'interface est utilisée en local sur `http://127.0.0.1:5003` et
aucun reverse proxy n'est installé sur la machine (`nginx`, `caddy` et
`cloudflared` sont absents). Il n'y a donc pas encore de « trafic public » à
basculer.

Deux cas concrets :

1. **Usage local seulement** : la bascule consiste à ouvrir
   `http://127.0.0.1:5004` au lieu de `http://127.0.0.1:5003`. Le rollback est
   immédiat : rouvrir 5003. Aucun proxy n'est nécessaire.
2. **Accès par nom de domaine ou depuis l'extérieur** : installer le proxy
   décrit ci-dessous, puis basculer la ligne `workflow_active`. C'est cette
   procédure qui rend la bascule réversible sans changer les URL des clients.

Le reste du document décrit le cas 2. Il ne suppose aucune modification du
code Python : celui-ci continue d'écouter sur 5003.

Attention au binding avant d'exposer le proxy : Rust écoute déjà sur
`127.0.0.1`, mais Flask utilise `FLASK_HOST` avec un défaut `0.0.0.0` (la
variable `HOST` du `.env` n'est pas lue par `config/settings.py`). Pour que
5003 ne soit pas joignable directement, démarrer Python avec
`FLASK_HOST=127.0.0.1` et vérifier :

```bash
ss -ltnp | grep -E ':5003|:5004'
# attendu : 127.0.0.1:5003 et 127.0.0.1:5004, jamais 0.0.0.0
```

## 1. Pourquoi un proxy

Les deux applications ne partagent ni état, ni base, ni racine de travail. Elles
ne peuvent donc pas répondre alternativement à la même session : le frontend
polling lirait tantôt l'état Python, tantôt l'état Rust. Le proxy doit choisir
**un backend actif** pour tout le bloc workflow, et basculer ce bloc d'un seul
coup.

Les routes qui n'existent qu'en Python restent routées vers 5003 en permanence.
C'est le cas de `/api/audio/*`, `/api/cache/*`, `/api/stats/*`,
`/api/system/diagnostics`, `/api/system_monitor`, `/api/performance/metrics`,
`/api/performance/reset`, `/test-sound` et `/test-slideshow-fixes`.

## 2. Table de routage

| Route | Backend |
| --- | --- |
| `/` , `/static/*`, `/favicon.ico` | actif (Rust après bascule) |
| `/sound-design/*` | actif (Rust) |
| `/run/*`, `/status/*`, `/stop/*`, `/cancel/*`, `/sequence/*`, `/get_specific_log/*`, `/run_custom_sequence` | actif |
| `/api/ping`, `/api/step_status/*`, `/api/csv_*`, `/api/performance/frontend` | actif |
| `/api/audio/*` | Python |
| `/api/cache/*` | Python |
| `/api/stats/*` | Python |
| `/api/system/diagnostics` | Python |
| `/api/system_monitor` | Python (données réelles) |
| `/api/performance/metrics`, `/api/performance/reset` | Python |
| `/test-sound`, `/test-slideshow-fixes`, `/get_specific_log_test/*` | Python |

`/api/system_monitor` est volontairement laissé à Python : le serveur Rust
expose un stub de polling, mais les métriques réelles viennent du service
Python. `/api/performance/frontend` reste sur Rust : le frontend ignore la
réponse et le stub Rust évite un 404.

Les routes Python-only lisent les racines Python (`projets_extraits`,
`cache`, base d'historique). Pendant que Rust est actif, elles ne décrivent
donc **pas** l'état des projets Rust : ce sont des outils d'exploitation
legacy, pas des routes de statut du workflow. Le frontend ne les appelle pas ;
seul `/api/system_monitor` est pollé et reste machine-wide.

## 3. Configuration Nginx

Trois fichiers suffisent. Le premier contient les upstreams fixes :

```nginx
# /etc/nginx/conf.d/00-workflow-upstreams.conf
upstream workflow_python {
    server 127.0.0.1:5003;
    keepalive 16;
}

upstream workflow_rust {
    server 127.0.0.1:5004;
    keepalive 16;
}
```

Le backend actif est isolé dans son propre fichier pour que la bascule ne
touche jamais l'upstream Python :

```nginx
# /etc/nginx/conf.d/00-workflow-active.conf
upstream workflow_active {
    server 127.0.0.1:5004;   # 5003 pour le rollback
    keepalive 16;
}
```

Le troisième contient le site public :

```nginx
# /etc/nginx/conf.d/10-workflow-site.conf
server {
    listen 80;
    server_name workflow.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    server_name workflow.example.com;

    ssl_certificate     /etc/letsencrypt/live/workflow.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/workflow.example.com/privkey.pem;

    # Routes Python-only, y compris pendant la bascule.
    location ~ ^/api/(audio/|cache/|stats/|system/diagnostics|system_monitor|performance/metrics|performance/reset) {
        proxy_pass http://workflow_python;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 600s;
        proxy_buffering off;
    }

    location = /test-sound {
        proxy_pass http://workflow_python;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 600s;
        proxy_buffering off;
    }

    location = /test-slideshow-fixes {
        proxy_pass http://workflow_python;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 600s;
    }

    # Route de diagnostic conservée côté Python.
    location ^~ /get_specific_log_test/ {
        proxy_pass http://workflow_python;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # Tout le reste suit le backend actif.
    location / {
        proxy_pass http://workflow_active;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        # Les séquences STEP2-STEP4 peuvent durer plusieurs minutes.
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        proxy_buffering off;
    }
}
```

Vérifier et recharger :

```bash
sudo nginx -t
sudo systemctl reload nginx
```

## 4. Canari avant la bascule

Deux méthodes concrètes, sans toucher au site principal :

1. **Hôte de test** : créer `rust.example.com` avec le même certificat et un
   `location / { proxy_pass http://workflow_rust; }`, puis valider l'interface
   complète et une séquence courte. C'est la méthode recommandée : elle teste
   le vrai frontend, les cookies, le polling et les routes Python-only.
2. **En-tête de test** : ajouter un `map` qui route les requêtes portant
   `X-Workflow-Backend: rust` vers 5004, puis tester avec `curl`. Le navigateur
   n'envoie pas cet en-tête par défaut ; cette méthode ne valide donc que
   l'API.

Pendant le canari, Python reste le backend actif du site principal.

## 5. Procédure de bascule

### Pré-vol

```bash
# 1. Les deux serveurs répondent en local.
curl -fs -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" \
  http://127.0.0.1:5004/api/ping
curl -fs -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" \
  http://127.0.0.1:5003/api/ping

# 2. Aucune séquence en cours.
curl -fs http://127.0.0.1:5004/sequence/status
curl -fs http://127.0.0.1:5003/sequence/status

# 3. Historique Rust à jour (réimporter si Python a ajouté des URL).
cd /home/kidpixel/workflow-rs
./target/release/workflow-cli history import \
  --from /home/kidpixel/workflow_mediapipe/download_history.sqlite3
./target/release/workflow-cli history audit
./target/release/workflow-cli webhook probe --plan   # doit rester à 0 ou aux seules nouvelles archives

# 4. Un seul moniteur CSV actif.
#    - soit Python garde /api/csv_* et son moniteur, et Rust reste webhook.enabled=false ;
#    - soit Rust prend /api/csv_*, on arrête le moniteur Python et on active Rust.
```

Ne jamais basculer au milieu d'une séquence : l'état d'exécution est en
mémoire et n'est pas transféré.

### Bascule

```bash
# Réécrire uniquement le backend actif.
sudo tee /etc/nginx/conf.d/00-workflow-active.conf >/dev/null <<'EOF'
upstream workflow_active {
    server 127.0.0.1:5004;
    keepalive 16;
}
EOF
sudo nginx -t && sudo systemctl reload nginx
```

### Vérification

```bash
curl -fs https://workflow.example.com/ | head
curl -fs -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" \
  https://workflow.example.com/api/ping
curl -fs https://workflow.example.com/api/csv_monitor_status
curl -fs https://workflow.example.com/sound-design/<fichier> -o /dev/null
# /api/audio/lemonfox est un POST protégé : attendre 400/401/200, jamais 404.
curl -s -o /dev/null -w '%{http_code}\n' -X POST \
  -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" \
  https://workflow.example.com/api/audio/lemonfox
curl -fs https://workflow.example.com/test-sound -o /dev/null
curl -fs https://workflow.example.com/test-slideshow-fixes -o /dev/null
```

Puis lancer une étape courte (STEP1 sur une archive de test, ou une séquence
STEP5/6 sur un projet existant) et vérifier `/status/<step>` et les logs.

## 6. Rollback

```bash
# 1. Arrêter la séquence Rust en cours, si nécessaire.
curl -fs -X POST -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" \
  http://127.0.0.1:5004/sequence/stop

# 2. Désactiver le moniteur Rust si Python reprend /api/csv_*.
#    config/config.toml : webhook.enabled = false, puis redémarrer Rust.

# 3. Remettre Python comme backend actif.
sudo tee /etc/nginx/conf.d/00-workflow-active.conf >/dev/null <<'EOF'
upstream workflow_active {
    server 127.0.0.1:5003;
    keepalive 16;
}
EOF
sudo nginx -t && sudo systemctl reload nginx
```

Python n'a pas été arrêté : le rollback est immédiat. Les projets et
téléchargements créés par Rust restent dans `runtime/` ; Python ne les voit pas.
Un rollback ne transfère donc pas l'état Rust vers Python. Il faut terminer ou
abandonner le projet Rust en cours avant de basculer.

## 7. DNS et TLS

- Si le proxy reste sur la même machine, **aucun changement DNS** n'est
  nécessaire : seule la ligne `workflow_active` change.
- Si le proxy déménage, baisser le TTL de l'enregistrement A/AAAA à 60 s,
  installer le certificat sur la nouvelle machine, vérifier
  `https://<hôte>/api/ping`, puis changer l'enregistrement. Rollback : remettre
  l'ancienne adresse et attendre l'expiration du TTL.
- Le certificat doit couvrir le nom public. Les backends restent en HTTP sur
  `127.0.0.1` ; ne pas exposer 5003/5004.
- Pare-feu : autoriser 80/443 depuis l'extérieur, refuser 5003/5004.

## 8. Déclencheurs de rollback

- erreurs 5xx ou timeouts sur les routes workflow ;
- interface Rust incohérente (étapes bloquées, polling qui ne progresse plus) ;
- séquence STEP1-STEP8 en échec reproductible ;
- moniteur CSV qui ne voit pas les nouveaux liens ou qui en télécharge trop ;
- erreur sur une route Python-only mal routée.

Le rollback ne touche ni la base Python ni ses racines. Il remet simplement le
trafic workflow sur 5003.

## 9. Checklist

- [ ] Python 5003 et Rust 5004 répondent en local.
- [ ] Aucune séquence en cours sur les deux backends.
- [ ] Historique Rust réimporté et audité.
- [ ] Un seul moniteur CSV actif.
- [ ] Canari `rust.example.com` validé (interface + une étape).
- [ ] `nginx -t` passe.
- [ ] Routes Python-only vérifiées après la bascule.
- [ ] Procédure de rollback testée au moins une fois.
