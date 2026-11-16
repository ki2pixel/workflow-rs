## **1\. Synthèse exécutive**

Le backend Rust workflow-rs (workspace de 9 crates ciblant Rust 1.86, édition 2021\) présente un niveau de maturité élevé et assure la production depuis le 23 octobre 2026 en remplacement du pipeline Python workflow\_mediapipe. L'architecture sépare clairement la configuration et l'isolation (workflow-core), les schémas et arrondis déterministes (workflow-schema), les 8 étapes métier et le post-traitement A5/A6/A7 (workflow-steps), l'ingestion et le cache SQLite (workflow-ingest), la synchronisation inter-backends (workflow-sync), le serveur d'orchestration Axum sur le port :5004 (workflow-server), le pont After Effects (workflow-bridge), la couche FFI Coral Edge TPU (workflow-tpu) et l'entrée opérationnelle unique (workflow-cli).  
**Score de maturité global : 8,4 / 10 — Statut : Conforme avec réserves (0 bloquant P0, 3 majeurs P1, 5 mineurs P2, 3 informatifs P3).**

### **Conformité aux exigences architecturales, de parité et de sécurité**

> * **Isolation et garde-fous de configuration (workflow-core)** : Les structures TOML appliquent \#\[serde(default, deny\_unknown\_fields)\]. validate\_server() bloque le port 5003 (réservé au legacy Python), rejette tout verrou nommé workflow-hw.lock, exige server.allow\_remote \= true pour une écoute hors loopback, et refuse les jetons worker\_token vides, par défaut ou préfixés par dev-. validate\_isolation() garantit par canonicalisation que history\_db, projects, archives, logs et downloads résident sous paths.base et jamais sous la racine Python legacy.  
> * **Parité fonctionnelle et contrats de données (workflow-steps, workflow-schema, parity)** : Les 23 cas de la baseline de parité et les validations shadow STEP5/STEP6 (57/57 puis 162/162 vérifications) confirment la parité à 1e-6 près, le maintien de l'ordre des clés JSON (preserve\_order) et le respect des quirks documentés (D-001 à D-027, et D-051 à D-069 pour A1/A2/A7/QA).  
> * **Axes de vulnérabilité et de fragilité identifiés** : L'audit du code source met en évidence un contournement possible du filtre anti-SSRF sur les adresses IP littérales dans Downloader (workflow-ingest), des appels SQLite synchrones sous std::sync::Mutex avec .expect() au sein de fonctions async, une absence de sync\_all() avant renommage final des archives téléchargées, un risque de perte silencieuse lors du découpage UTF-8 par longueur d'octets dans parse\_lenient\_logos, ainsi qu'un piège d'isolation lié aux chemins absolus codés en dur dans config/config.toml.

## **2\. Vérification par composant (Étapes 2 à 4 du protocole)**

### **workflow-core & workflow-schema**

> * **Configuration et validation** : Config::from\_toml rejette toute clé inconnue et applique les variables d'environnement via apply\_env\_lookup, qui échoue explicitement sur toute valeur entière ou booléenne mal formée.  
> * **Verrou matériel** : HardwareLock::acquire (crates/workflow-core/src/lock.rs) utilise fs2::FileExt::lock\_exclusive / try\_lock\_exclusive sur /tmp/workflow-rs-hw.lock et libère le verrou de manière déterministe dans Drop.  
> * **Écritures atomiques et nettoyage** : fsutil::write\_atomic écrit dans un fichier caché .\<name\>.tmp.\<pid\>, exécute flush(), sync\_all(), ferme le descripteur, renomme vers la cible et synchronise le répertoire parent via sync\_parent. purge\_orphans supprime les résidus \_temp\_\*, \*.part et \*.tmp\* plus anciens que max\_age\_hours sans suivre les liens symboliques.

### **workflow-server & workflow-cli**

> * **Sérialisation et cycle de vie des étapes** : Le serveur :5004 sérialise les exécutions (renvoyant HTTP 409 en cas de second lancement simultané), conserve le créneau pendant l'état stopping (envoi de SIGTERM puis SIGKILL au groupe de processus) et ignore les fins d'exécution périmées grâce à RunId.  
> * **Gestion des signaux CLI** : workflow-cli installe un gestionnaire SIGTERM/SIGINT (workflow\_steps::process::install\_shutdown\_handler()) pour Step1, Step2, Step3 et Step4 afin de propager l'arrêt aux sous-processus enfants (ffmpeg, unrar), tandis que Step8 gère son propre watchdog et son groupe de processus effectcraft-cli.  
> * **Masquage des secrets** : La sous-commande workflow-cli config show remplace server.secret\_key et server.worker\_token par "\*\*\*" avant sérialisation JSON.

### **workflow-steps & Sous-système Post-traitement (A1–A7 / QA)**

> * **STEP1 à STEP4** : STEP1 valide et assainit les chemins d'archives, applique les plafonds anti-zip-bomb (max\_extracted\_bytes \= 100 Gio, max\_compression\_ratio \= 200), normalise les vidéos en 25 fps progressif (yadif) et préserve à l'octet près les .mov alpha et m6+.mov (D-010, D-016). STEP2 exécute TransNetV2 sous ort CUDA (use\_tf32 \= false, cudnn\_algo\_search \= true) avec fenêtres 100/50/25. STEP3 gère l'absence de piste audio par un SKIP\_NO\_AUDIO propre en code 0 (D-002). STEP4 exige par défaut \<stem\>.csv et \<stem\>\_audio.json (D-004) et écrit son JSON de manière atomique (D-008).  
> * **STEP5 & STEP6** : Les réducteurs opèrent en streaming sous le plafond de 100 Mio de RSS (16 à 44 Mio mesurés sur les projets réels) et écrivent fps et total\_frames avant frames\_analysis (D-003).  
> * **STEP7, STEP8 & Post-traitement (A1–A7)** : STEP7 archive par SHA-256, publie vers paths.finalized et génère finalized\_projects\_\<ts\>.json (avec \--no-delete et \--dry-run supportés). STEP8 génère le projet brut .ecproj via effectcraft-cli puis déclenche le post-traitement qui publie \<stem\>\_assist.ecproj sans altérer le brut. Le veto pillarbox latéral A1 (D-063), le registre spatial des logos A2 (a2\_logo\_prior, D-052/D-062), la banque centrale A7 avec repli sur corruption locale (D-066/D-069) et le sidecar \<stem\>\_mission.json en schema: 2 (D-065) sont cablés et validés.

### **workflow-ingest, workflow-sync & workflow-tpu**

> * **SQLite (rusqlite)** : Toutes les requêtes SQL de HistoryRepository (crates/workflow-ingest/src/history.rs) et EmailMissionStore (crates/workflow-ingest/src/email.rs) utilisent exclusivement des requêtes préparées et paramétrées (params\!\[...\]), avec PRAGMA journal\_mode=WAL, PRAGMA synchronous=NORMAL et busy\_timeout(30s). L'import depuis Python ouvre la base source avec SQLITE\_OPEN\_READ\_ONLY et vérifie que les chemins canoniques source et destination diffèrent.  
> * **Synchronisation (workflow-sync)** : La commande workflow-cli sync interdit toute combinaison \--apply avec \--allow-running, exige que les ports 5003 et 5004 soient arrêtés, acquiert /tmp/workflow-sync.lock, et crée une sauvegarde horodatée sous runtime/backups/sync/\<horodatage\> avant écriture.  
> * **FFI Edge TPU (workflow-tpu)** : Le chargement dynamique de libtensorflowlite\_c.so et libedgetpu.so.1 valide la présence de l'opérateur edgetpu-custom-op et a été éprouvé sur 10 000 invocations consécutives sans fuite mémoire au bit près.

## **3\. Tableau récapitulatif des constats**

| ID | Crate / Composant | Catégorie | Sévérité | Titre |
| :---- | :---- | :---- | :---- | :---- |
| **AUD-01** | workflow-ingest (download.rs) | Sécurité | **P1 Majeur** | Contournement potentiel du filtre anti-SSRF (PublicOnlyResolver) par adresse IP littérale |
| **AUD-02** | workflow-ingest (history.rs, email.rs) | Concurrence / Robustesse | **P1 Majeur** | Verrouillage std::sync::Mutex avec .expect() et I/O SQLite bloquantes dans des contextes async |
| **AUD-03** | workflow-ingest (download.rs) | Robustesse | **P1 Majeur** | Absence de sync\_all() sur le fichier .part et le dossier parent avant renommage final de l'archive |
| **AUD-04** | workflow-core (mission.rs) | Robustesse | **P2 Mineur** | Découpage de chaîne UTF-8 par longueur d'octets après to\_lowercase() dans parse\_lenient\_logos |
| **AUD-05** | config/config.toml & workflow-core | Sécurité / Exploitation | **P2 Mineur** | Webhook actif par défaut et surcharge partielle de WORKFLOW\_PATHS\_BASE face aux chemins absolus du TOML |
| **AUD-06** | workflow-core (fsutil.rs, lock.rs) | Robustesse | **P2 Mineur** | Correspondance trop large .contains(".tmp") dans purge\_orphans et absence de PID dans HardwareLock |
| **AUD-07** | workflow-cli (main.rs) | Performance / Dette | **P2 Mineur** | Instanciation répétée de runtimes Tokio complets et détection de suffixe sensible à la casse |
| **AUD-08** | workflow-core & assets/web | Sécurité | **P2 Mineur** | secret\_key non validée côté serveur et exposition du worker-token dans le DOM HTML en cas d'accès distant |
| **AUD-09** | workflow-steps & workflow-server | Dette | **P3 Info** | Dérogations architecturales résiduelles documentées (D-025 code HTTP 404, D-026 anyhow interne) |
| **AUD-10** | workflow-core (mission.rs) & workflow-ingest (download.rs) | Performance | **P3 Info** | Allocations quadratiques (Vec::contains sur clés dédupliquées) et lecture complète en mémoire dans parse\_eml |

## **4\. Analyse détaillée par constat**

### **AUD-01 — Contournement potentiel du filtre anti-SSRF (PublicOnlyResolver) par adresse IP littérale**

> * **Catégorie** : Sécurité \| **Sévérité** : **P1 Majeur**  
> * **Localisation** : /home/kidpixel/workflow-rs/crates/workflow-ingest/src/download.rs (lignes d'implémentation de PublicOnlyResolver et Downloader::new).  
> * **Extrait de code incriminé** :

impl Resolve for PublicOnlyResolver {  
    fn resolve(\&self, name: Name) \-\> Resolving {  
        Box::pin(async move {  
            let host \= name.as\_str().to\_string();  
            let resolved \= tokio::net::lookup\_host((host.as\_str(), 0)).await?;  
            let addresses: Vec\<SocketAddr\> \= resolved  
                .filter(\|address\| is\_public\_ip(address.ip()))  
                .collect();  
            ...  
        })  
    }  
}

> * **Impact potentiel en production** : Dans reqwest / hyper, le trait Resolve (dns\_resolver) n'est appelé que lorsqu'une URL comporte un nom de domaine à résoudre par DNS. Si le flux du webhook ou une redirection HTTP 301/302 fournit une URL contenant directement une adresse IPv4 ou IPv6 littérale privée ou loopback (par exemple \[http\://127.0.0.1:5003/\](http\://127.0.0.1:5003/)... ou \[http\://192.168.1.1/\](http\://192.168.1.1/)...), hyper se connecte directement à l'adresse IP sans invoquer PublicOnlyResolver, contournant la protection allow\_private\_hosts \= false.  
> * **Recommandation de remédiation** : Valider explicitement l'hôte de l'URL initiale ainsi que chaque cible de redirection dans une reqwest::redirect::Policy::custom en testant si url.host() est une adresse IPv4/IPv6 non publique via is\_public\_ip avant toute connexion :

fn validate\_url\_host(url: \&reqwest::Url, allow\_private: bool) \-\> Result\<(), io::Error\> {  
    if allow\_private {  
        return Ok(());  
    }  
    match url.host() {  
        Some(url::Host::Ipv4(ip)) if \!is\_public\_ipv4(ip) \=\> Err(io::Error::new(  
            io::ErrorKind::PermissionDenied,  
            format\!("IP littérale non publique refusée: {ip}"),  
        )),  
        Some(url::Host::Ipv6(ip)) if \!is\_public\_ip(IpAddr::V6(ip)) \=\> Err(io::Error::new(  
            io::ErrorKind::PermissionDenied,  
            format\!("IPv6 littérale non publique refusée: {ip}"),  
        )),  
        None \=\> Err(io::Error::new(io::ErrorKind::InvalidInput, "hôte absent")),  
        \_ \=\> Ok(()),  
    }  
}

### **AUD-02 — Verrouillage std::sync::Mutex avec .expect() et I/O SQLite bloquantes dans des contextes async**

> * **Catégorie** : Concurrence / Robustesse \| **Sévérité** : **P1 Majeur**  
> * **Localisation** :  
  * /home/kidpixel/workflow-rs/crates/workflow-ingest/src/history.rs (HistoryRepository::with\_connection)  
  * /home/kidpixel/workflow-rs/crates/workflow-ingest/src/email.rs (EmailMissionStore::with\_connection, EmailProxyClient::index, EmailProxyClient::sync)  
> * **Extrait de code incriminé** :

pub fn with\_connection\<T\>(  
    \&self,  
    action: impl FnOnce(\&Connection) \-\> rusqlite::Result\<T\>,  
) \-\> IngestResult\<T\> {  
    let connection \= self.connection.lock().expect("base email verrouillée");  
    action(\&connection).map\_err(\|error\| IngestError::Message(format\!("SQLite email: {error}")))  
}

> * **Impact potentiel en production** :  
  1. **Blocage des workers Tokio** : Les connexions SQLite sont configurées avec busy\_timeout(Duration::from\_secs(30)). Lorsqu'une fonction async (comme EmailProxyClient::sync ou le moniteur d'ingestion) appelle store.needs\_detail() ou store.upsert() directement sur le thread de l'exécuteur Tokio sans spawn\_blocking, toute contention SQLite ou écriture disque lente bloque le thread Tokio jusqu'à 30 secondes (violation de .agents/skills/rust-concurrency-orchestrator/SKILL.md et .agents/rules/architecte.md).  
  2. **Propagation de panique (PoisonError)** : L'utilisation de std::sync::Mutex::lock().expect(...) provoque un crash immédiat si un thread a paniqué précédemment en tenant le verrou, alors que parking\_lot::Mutex est déjà déclaré dans les dépendances du workspace (Cargo.toml).  
> * **Recommandation de remédiation** : Remplacer std::sync::Mutex par parking\_lot::Mutex (qui ne s'empoisonne pas et élimine les .expect()), et encapsuler les opérations SQLite lourdes ou appelées depuis un contexte async dans tokio::task::spawn\_blocking :

use parking\_lot::Mutex;

pub fn with\_connection\<T\>(  
    \&self,  
    action: impl FnOnce(\&Connection) \-\> rusqlite::Result\<T\>,  
) \-\> IngestResult\<T\> {  
    let connection \= self.connection.lock();  
    action(\&connection).map\_err(\|error\| IngestError::Message(format\!("SQLite email: {error}")))  
}

### **AUD-03 — Absence de sync\_all() sur le fichier .part et le dossier parent avant renommage final de l'archive**

> * **Catégorie** : Robustesse \| **Sévérité** : **P1 Majeur**  
> * **Localisation** : /home/kidpixel/workflow-rs/crates/workflow-ingest/src/download.rs (stream\_to\_part et finalize\_download).  
> * **Extrait de code incriminé** :

if let Err(error) \= file.flush().await {  
    return local\_io\_failure(error, effective\_offset \+ written);  
}  
...  
if let Err(error) \= fs::rename(part\_path, target\_path).await {  
    return (  
        false,  
        ErrorKind::LocalIo,  
        format\!("Unable to finalize {}: {error}", ...),  
    );  
}

> * **Impact potentiel en production** : file.flush().await vide uniquement le tampon utilisateur Rust vers le cache du noyau OS sans garantir l'écriture physique sur le disque (fsync). Dès que finalize\_download renomme .part vers .zip, le moniteur enregistre l'URL dans download\_history (avec PRAGMA synchronous=NORMAL en WAL). En cas de coupure secteur ou de crash noyau, l'entrée SQLite peut survivre tandis que l'archive .zip dans runtime/downloads est de taille nulle ou tronquée, empêchant à la fois son extraction par STEP1 et son retéléchargement automatique.  
> * **Recommandation de remédiation** : Appeler file.sync\_all().await après file.flush().await dans stream\_to\_part, et appeler workflow\_core::fsutil::sync\_parent(target\_path) après fs::rename(part\_path, target\_path).await dans finalize\_download :

if let Err(error) \= file.flush().await {  
    return local\_io\_failure(error, effective\_offset \+ written);  
}  
if let Err(error) \= file.sync\_all().await {  
    return local\_io\_failure(error, effective\_offset \+ written);  
}

### **AUD-04 — Découpage de chaîne UTF-8 par longueur d'octets après to\_lowercase() dans parse\_lenient\_logos**

> * **Catégorie** : Robustesse \| **Sévérité** : **P2 Mineur**  
> * **Localisation** : /home/kidpixel/workflow-rs/crates/workflow-core/src/mission.rs (parse\_lenient\_logos).  
> * **Extrait de code incriminé** :

for label in \[  
    "chaîne",  
    "chaine",  
    "émission",  
    "emission",  
    "programme",  
    "demandé",  
    "demande",  
    "animé",  
    "anime",  
\] {  
    let lower \= rest.to\_lowercase();  
    if lower.starts\_with(label) {  
        rest \= rest.get(label.len()..).unwrap\_or\_default().trim();  
        break;  
    }  
}

> * **Impact potentiel en production** : label.len() est calculé en octets UTF-8 sur la chaîne minuscule label, puis appliqué comme index d'octet sur rest (avant mise en minuscule). Si rest contient un caractère Unicode dont la représentation majuscule n'a pas le même nombre d'octets UTF-8 que sa représentation minuscule, rest.get(label.len()..) tombe hors d'une frontière de caractère UTF-8, retourne None, et .unwrap\_or\_default() vide silencieusement rest, faisant basculer l'instruction en logo\_unresolved.  
> * **Recommandation de remédiation** : Découper par nombre de caractères (chars().count()) plutôt que par index d'octets :

let lower \= rest.to\_lowercase();  
for label in \[  
    "chaîne", "chaine", "émission", "emission",  
    "programme", "demandé", "demande", "animé", "anime",  
\] {  
    if lower.starts\_with(label) {  
        let char\_count \= label.chars().count();  
        let byte\_offset \= rest  
            .char\_indices()  
            .nth(char\_count)  
            .map(\|(idx, \_)\| idx)  
            .unwrap\_or(rest.len());  
        rest \= rest\[byte\_offset..\].trim();  
        break;  
    }  
}

### **AUD-05 — Webhook actif par défaut et surcharge partielle de WORKFLOW\_PATHS\_BASE face aux chemins absolus du TOML**

> * **Catégorie** : Sécurité / Exploitation \| **Sévérité** : **P2 Mineur**  
> * **Localisation** : /home/kidpixel/workflow-rs/config/config.toml et /home/kidpixel/workflow-rs/crates/workflow-core/src/config.rs (apply\_env\_lookup).  
> * **Extrait de code incriminé** :

\[paths\]  
base \= "/home/kidpixel/workflow-rs/runtime"  
projects \= "/home/kidpixel/workflow-rs/runtime/projets\_extraits"  
archives \= "/home/kidpixel/workflow-rs/runtime/archives"  
logs \= "/home/kidpixel/workflow-rs/runtime/logs"  
downloads \= "/home/kidpixel/workflow-rs/runtime/downloads"  
history\_db \= "/home/kidpixel/workflow-rs/runtime/download\_history.sqlite3"

if let Some(value) \= env\_first(\&lookup, &\["WORKFLOW\_PATHS\_BASE"\]) {  
    self.paths.base \= PathBuf::from(value);  
}

> * **Impact potentiel en production** :  
  1. Dans config/config.toml, projects, archives, logs, downloads et history\_db sont définis explicitement en absolu (Some(...)). Or PathsConfig::resolve() n'utilise base.join(...) que lorsque ces champs valent None. Si un opérateur ou un script de test exporte WORKFLOW\_PATHS\_BASE=/tmp/test en passant \--config config/config.toml (comme le fait scripts/validate-step1.py qui doit réécrire le texte du TOML par remplacement de chaîne), paths.projects continuerait de pointer vers /home/kidpixel/workflow-rs/runtime/... si le remplacement textuel échouait.  
  2. webhook.enabled \= true et email\_proxy.enabled \= true sont activés dans le fichier config/config.toml versionné, ce qui expose tout lancement accidentel sans copie de configuration à interroger les endpoints de production.  
> * **Recommandation de remédiation** : Dans config/config.toml, omettre projects, archives, logs, downloads et history\_db lorsqu'ils correspondent aux sous-dossiers standards de paths.base afin que PathsConfig::resolve() les dérive dynamiquement de paths.base (et donc de WORKFLOW\_PATHS\_BASE).

### **AUD-06 — Correspondance trop large .contains(".tmp") dans purge\_orphans et absence de PID dans HardwareLock**

> * **Catégorie** : Robustesse \| **Sévérité** : **P2 Mineur**  
> * **Localisation** :  
  * /home/kidpixel/workflow-rs/crates/workflow-core/src/fsutil.rs (purge\_orphans\_with)  
  * /home/kidpixel/workflow-rs/crates/workflow-core/src/lock.rs (HardwareLock::acquire)  
> * **Extrait de code incriminé** :

let is\_temp\_dir \= file\_type.is\_dir() && name.starts\_with("\_temp\_");  
let is\_partial \=  
    file\_type.is\_file() && (name.ends\_with(".part") \|\| name.contains(".tmp"));

> * **Impact potentiel en production** :  
  1. hidden\_temp\_path nomme les fichiers temporaires selon le patron .{name}.tmp.{pid}, mais purge\_orphans\_with cible tout fichier régulier dont le nom contient .tmp (name.contains(".tmp")). Un fichier de projet ou d'archive nommé par un client avec la sous-chaîne .tmp (même sans être caché par un . initial) et vieux de plus de orphan\_max\_age\_hours (24 h) serait supprimé au démarrage du serveur.  
  2. HardwareLock::acquire ne renseigne pas le PID du processus courant dans /tmp/workflow-rs-hw.lock après acquisition du verrou flock (contrairement à acquire\_hardware\_lock dans tools/parity/parity.py), rendant l'inspection d'un verrou bloqué moins directe.  
> * **Recommandation de remédiation** : Restreindre le filtre is\_partial dans fsutil.rs aux patrons exacts générés par le pipeline (name.ends\_with(".part") \|\| name.ends\_with(".tmp") \|\| (name.starts\_with('.') && name.contains(".tmp."))), et écrire std::process::id() dans le fichier de verrou lors de HardwareLock::acquire.

### **AUD-07 — Instanciation répétée de runtimes Tokio complets et détection de suffixe sensible à la casse dans workflow-cli**

> * **Catégorie** : Performance / Dette \| **Sévérité** : **P2 Mineur**  
> * **Localisation** : /home/kidpixel/workflow-rs/crates/workflow-cli/src/main.rs (default\_video\_output, load\_missions\_for\_folders, Command::Webhook, Command::Email, Command::Download).  
> * **Extrait de code incriminé** :

fn default\_video\_output(input: \&std::path::Path) \-\> PathBuf {  
    let name \= input  
        .file\_name()  
        .and\_then(\|name\| name.to\_str())  
        .unwrap\_or("");  
    if name.ends\_with("\_tracking.json") {  
        return input.to\_path\_buf();  
    }  
    ...  
}

> * **Impact potentiel en production** :  
  1. default\_ae\_output normalise en minuscules (let lower \= name.to\_ascii\_lowercase(); if lower.ends\_with("\_tracking.json")), tandis que default\_video\_output teste name.ends\_with("\_tracking.json") de façon sensible à la casse : un fichier nommé VIDEO\_TRACKING.JSON produirait VIDEO\_TRACKING\_tracking.json.  
  2. tokio::runtime::Runtime::new() crée un pool multi-thread complet à chaque sous-commande réseau ou dans load\_missions\_for\_folders, alors qu'un runtime Builder::new\_current\_thread().enable\_all().build() suffit pour les appels séquentiels du CLI (probe, sync, detail, download).  
> * **Recommandation de remédiation** : Aligner default\_video\_output sur to\_ascii\_lowercase() comme default\_ae\_output, et factoriser un helper block\_on\_cli(future) utilisant tokio::runtime::Builder::new\_current\_thread().enable\_all().build().

### **AUD-08 — secret\_key non validée côté serveur et exposition du worker-token dans le DOM HTML en cas d'accès distant**

> * **Catégorie** : Sécurité \| **Sévérité** : **P2 Mineur**  
> * **Localisation** :  
  * /home/kidpixel/workflow-rs/crates/workflow-core/src/config.rs (validate\_server)  
  * /home/kidpixel/workflow-rs/assets/web/static/apiService.js (fetchWithLoadingState)  
> * **Extrait de code incriminé** :

const tokenEl \= document.querySelector('meta\[name="worker-token"\]');  
const token \= tokenEl ? tokenEl.getAttribute('content') : '';  
if (token) {  
    options.headers \= {  
        ...options.headers,  
        'X-Worker-Token': token  
    };  
}

> * **Impact potentiel en production** : Le frontend legacy lit le jeton X-Worker-Token directement depuis \<meta name="worker-token"\> injecté dans le template HTML servi sur GET /. Tant que server.host \= "127.0.0.1" et server.allow\_remote \= false, seuls les processus locaux peuvent lire GET /. En revanche, si un opérateur active server.allow\_remote \= true sans authentification amont sur GET /, tout client réseau accédant à la page d'accueil récupère le worker\_token et peut invoquer les routes mutantes (/run/\*, /cancel/\*). De plus, server.secret\_key est acceptée vide par validate\_server().  
> * **Recommandation de remédiation** : Documenter et imposer dans validate\_server() que server.allow\_remote \= true exige une clé secret\_key non vide ou un reverse proxy authentifié (conformément à docs/exploitation/SWITCH-PUBLIC.md), et envisager le rejet de secret\_key préfixée par dev- lorsqu'elle est renseignée.

### **AUD-09 — Dérogations architecturales résiduelles documentées (D-025, D-026)**

> * **Catégorie** : Dette \| **Sévérité** : **P3 Info**  
> * **Localisation** : /home/kidpixel/workflow-rs/parity/DRIFT.md (entrées D-025 et D-026).  
> * **Description et impact** :  
  * **D-025** : ServeDir sur /sound-design retourne 404 au lieu de 403 {"error":"Access denied"} pour les chemins hors racine ; aucun fichier hors périmètre n'est divulgué.  
  * **D-026** : Les modules internes de workflow-steps (step2, step3, step4, step7, step8) retournent anyhow::Result au lieu d'un StepError typé avec thiserror, contrairement à workflow-ingest (IngestError) et workflow-sync (SyncError).  
> * **Recommandation de remédiation** : Maintenir l'interdiction d'introduire de nouvelles API publiques en anyhow::Result dans workflow-steps et migrer progressivement les points d'entrée d'étapes vers StepError lors des prochains chantiers.

### **AUD-10 — Allocations quadratiques (Vec::contains sur clés dédupliquées) et décodage en mémoire dans mission.rs / email.rs**

> * **Catégorie** : Performance \| **Sévérité** : **P3 Info**  
> * **Localisation** :  
  * /home/kidpixel/workflow-rs/crates/workflow-core/src/mission.rs (split\_logo\_tokens, extract\_delivery\_links)  
  * /home/kidpixel/workflow-rs/crates/workflow-cli/src/main.rs (audit\_missions)  
> * **Extrait de code incriminé** :

let mut seen: Vec\<String\> \= Vec::new();  
...  
if key.is\_empty() \|\| seen.contains(\&key) {  
    continue;  
}  
seen.push(key);

> * **Impact potentiel en production** : Négligeable sur la taille habituelle d'un email (quelques tokens par ligne), mais l'utilisation d'un HashSet\<String\> à la place de Vec\<String\>::contains dans split\_logo\_tokens, extract\_delivery\_links et audit\_missions est plus idiomatique et évite des recherches linéaires et clones inutiles.

## **5\. Plan d'action et priorités de refactorisation**

### **Actions immédiates (P1 — Sécurité, Concurrence et Intégrité des données)**

> 1. ✅ **Implémenté et testé (D-070)** — **Durcir la protection anti-SSRF de Downloader (AUD-01)** : Ajouter une validation de l'hôte IP littéral (is\_public\_ip) avant la requête initiale et dans une reqwest::redirect::Policy::custom au sein de crates/workflow-ingest/src/download.rs.  
> 2. ✅ **Implémenté et testé (D-070)** — **Sécuriser les accès SQLite et les verrous (AUD-02)** : Remplacer std::sync::Mutex par parking\_lot::Mutex dans HistoryRepository (history.rs), EmailMissionStore et EmailProxyClient (email.rs), et déporter les appels SQLite effectués depuis des fonctions async dans tokio::task::spawn\_blocking.  
> 3. ✅ **Implémenté et testé (D-070)** — **Garantir la durabilité disque des téléchargements (AUD-03)** : Ajouter file.sync\_all().await avant la fermeture du fichier .part et workflow\_core::fsutil::sync\_parent(target\_path) après fs::rename dans crates/workflow-ingest/src/download.rs.

### **État d'implémentation des actions P1 (2026-11-16)**

Les trois constats P1 sont corrigés dans `workflow-ingest` et couverts par la
suite unitaire (D-070) :

- **AUD-01** : `validate_url_host` refuse les IP littérales non publiques sur
  l'URL initiale (avant `probe` et avant tout `GET`) et
  `redirect_policy` réapplique le filtre à chaque redirection, avec la même
  borne de trois sauts que `Policy::limited(3)`. Un refus produit
  `ErrorKind::Auth`, sans connexion réseau ni fichier `.part`.
- **AUD-02** : `HistoryRepository`, `EmailMissionStore` et le cache d'index de
  `EmailProxyClient` utilisent `parking_lot::Mutex` (aucun empoisonnement,
  plus de `.expect()`). `needs_detail` et `upsert` de
  `EmailProxyClient::sync` passent par `tokio::task::spawn_blocking` ; un
  `JoinError` devient un `IngestError::Message` rapporté dans `report.errors`.
- **AUD-03** : `stream_to_part` exécute `file.sync_all().await` après `flush`
  et `finalize_download` appelle `workflow_core::fsutil::sync_parent` après le
  `rename` atomique ; l'échec de fsync du répertoire reste non bloquant.

**Validation** : `cargo test -p workflow-ingest` 55/55 ; suite workspace 678
tests listés (676 exécutés + 2 ignorés), 0 échec ; `cargo fmt --check` et
`cargo clippy --workspace --all-targets -- -D warnings` verts. Aucune écriture
dans `runtime/`, `/mnt/cache` ou le dépôt Python legacy.

### **Chantiers d'optimisation et de durcissement recommandés (P2 / P3)**

> 1. ✅ **Implémenté (D-071)** — **Corriger le découpage UTF-8 dans parse\_lenient\_logos (AUD-04)** : Utiliser char\_indices() au lieu de rest.get(label.len()..) dans crates/workflow-core/src/mission.rs et ajouter un test unitaire avec préfixe accentué majuscule ("Chaîne : gulli.png", "Émission : top chef.png").
> 2. ✅ **Implémenté (D-071)** — **Assainir la résolution des chemins dans config/config.toml (AUD-05)** : Laisser projects, archives, logs, downloads et history\_db se résoudre automatiquement depuis paths.base lorsque leurs noms standard sont utilisés, afin que WORKFLOW\_PATHS\_BASE suffise à isoler tout l'environnement.
> 3. ✅ **Implémenté (D-071)** — **Affiner purge\_orphans et HardwareLock (AUD-06)** : Cibler uniquement les fichiers temporaires cachés .\*.tmp.\* ou suffixés par .tmp/.part dans crates/workflow-core/src/fsutil.rs, et inscrire le PID courant dans HardwareLock::acquire (crates/workflow-core/src/lock.rs).
> 4. ✅ **Implémenté (D-071)** — **Uniformiser workflow-cli (AUD-07, AUD-10)** : Rendre default\_video\_output insensible à la casse, utiliser un runtime Tokio current\_thread léger pour les commandes CLI ponctuelles, et remplacer les Vec\<String\> de déduplication par HashSet\<String\>.

### **État d'implémentation des actions P2/P3 (2026-11-16)**

Les constats P2 et P3 sont traités (D-071) et couverts par la suite unitaire :

- **AUD-04** : `parse_lenient_logos` retire les préfixes par nombre de
  caractères (`strip_chars`, `char_indices()`), sans index d'octet ni
  troncature silencieuse. Tests : `Chaîne`, `Émission` et `Animé` en
  minuscules et majuscules, avec et sans espace avant `:`.
- **AUD-05** : `config/config.toml` ne déclare plus `projects`, `archives`,
  `logs`, `downloads`, `history_db` ni `steps.step1.source_dir`. Ils sont
  dérivés de `paths.base` via `PathsConfig::resolve()` et
  `Config::step1_source_dir()`, donc `WORKFLOW_PATHS_BASE` isole tout le
  runtime sans réécriture textuelle. `validate_isolation()` reste inchangé et
  `scripts/validate-step1.py` passe sur le TOML ainsi allégé. Le moniteur reste
  activé en production (`enabled = true`) : la recommandation retenue porte sur
  l'isolation, pas sur la désactivation.
- **AUD-06** : `purge_orphans_with` ne cible que `.part`, `.tmp` final et
  `.*.tmp.*` ; `HardwareLock::acquire` inscrit le PID courant après `flock`
  (`set_len`, `seek`, `flush`, `sync_all`). Tests : `projet.tmp.archive.zip`
  conservé, `.fichier.tmp.1234` purgé, contenu du verrou vérifié.
- **AUD-07 / AUD-10** : `default_video_output` normalise en ASCII minuscule ;
  `block_on_cli`/`block_on_cli_result` utilisent
  `Builder::new_current_thread().enable_all()` pour webhook, email,
  téléchargement et `load_missions_for_folders` ; `split_logo_tokens`,
  `extract_delivery_links` et `audit_missions` dédupliquent via `HashSet`.
- **AUD-08** : `validate_server` refuse `allow_remote = true` avec une
  `secret_key` vide, blanche ou préfixée `dev-` ; le rejet des `worker_token`
  invalides est conservé. Tests : secret absent, blanc ou `dev-` refusé,
  secret réel accepté.
- **AUD-09** : aucune nouvelle signature publique `anyhow::Result` dans
  `workflow-steps` ; les entrées D-025 et D-026 de `parity/DRIFT.md` sont
  vérifiées et datées du 2026-11-16.

**Validation** : `cargo test --workspace` 689 tests listés (687 exécutés +
2 ignorés), 0 échec ; `cargo fmt --check` et
`cargo clippy --workspace --all-targets -- -D warnings` verts. Aucune écriture
dans `runtime/`, `/mnt/cache` ou le dépôt Python legacy.
