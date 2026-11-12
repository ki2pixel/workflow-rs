### 1. Format de l'Index récapitulatif
> **URL :** `GET https://webhook.kidpixel.fr/email_proxy.php`  
> *(ou avec pagination : `GET /email_proxy.php?limit=10`)*

Ce point d'entrée renvoie un tableau JSON léger ordonné du plus récent au plus ancien (idéal pour le polling, l'affichage de listes ou la détection de nouveaux lots) :

```json
[
  {
    "id": "recadrage_lot_158_20261110",
    "lot": 158,
    "subject": "Média Solution - Missions Recadrage - Lot 158",
    "received_at": "2026-11-10T15:56:36+01:00",
    "stored_at": "2026-11-11T20:30:15+01:00",
    "deadline": "17h20",
    "urgent": false,
    "sender": "t***@media-solution.fr",
    "snippet": "Voici les missions pour le lot 158. Recadrage 9:16 standard à faire pour 17h20 (aujourd'hui 10/11) : - CABINE TELEPHONE_TAT_Téva.mp4 (Logos requis: teva.png)...",
    "file": "recadrage_lot_158_20261110.json",
    "links_count": 1,
    "size_bytes": 1845
  },
  {
    "id": "recadrage_lot_136_20261110",
    "lot": 136,
    "subject": "Média Solution - Missions Recadrage - Lot 136",
    "received_at": "2026-11-10T12:56:17+01:00",
    "stored_at": "2026-11-11T20:30:14+01:00",
    "deadline": "13h45",
    "urgent": false,
    "sender": "t***@media-solution.fr",
    "snippet": "Voici les missions pour le lot 136. Recadrage 9:16 standard à faire pour 13h45 (aujourd'hui 10/11) : - M6+ - THE 100 – Extrait 7 – Lexa Promesse.mp4...",
    "file": "recadrage_lot_136_20261110.json",
    "links_count": 1,
    "size_bytes": 1920
  },
  {
    "id": "recadrage_lot_131_20261110",
    "lot": 131,
    "subject": "Média Solution - Missions Recadrage - Lot 131",
    "received_at": "2026-11-10T11:04:16+01:00",
    "stored_at": "2026-11-11T20:30:12+01:00",
    "deadline": "11h50",
    "urgent": false,
    "sender": "t***@media-solution.fr",
    "snippet": "Voici les missions pour le lot 131. Recadrage 9:16 standard à faire pour 11h50 (aujourd'hui 10/11) : - SDM - Leslie et Léo.mp4 (Logos requis: sdm.png)...",
    "file": "recadrage_lot_131_20261110.json",
    "links_count": 1,
    "size_bytes": 1780
  },
  {
    "id": "recadrage_lot_80_20261106",
    "lot": 80,
    "subject": "Média Solution - Missions Recadrage - Lot 80 - URGENT",
    "received_at": "2026-11-06T10:13:22+01:00",
    "stored_at": "2026-11-11T20:30:10+01:00",
    "deadline": "13h25",
    "urgent": true,
    "sender": "t***@media-solution.fr",
    "snippet": "Voici les missions pour le lot 80. URGENT Traiter IMMÉDIATEMENT, à rendre au plus tard 11h00 (aujourd'hui 06/11) : - Leur belle-mère leur pourrit la vie !.mp4...",
    "file": "recadrage_lot_80_20261106.json",
    "links_count": 1,
    "size_bytes": 2150
  },
  {
    "id": "recadrage_lot_130_20261012",
    "lot": 130,
    "subject": "Média Solution - Missions Recadrage - Lot 130",
    "received_at": "2026-10-12T12:45:53+02:00",
    "stored_at": "2026-11-11T20:30:08+01:00",
    "deadline": "14h45",
    "urgent": false,
    "sender": "t***@media-solution.fr",
    "snippet": "Voici les missions pour le lot 130. Recadrage 9:16 standard à faire pour 14h45 (aujourd'hui 12/10) : - M6 - AAL - Il abandonne les flammes.mp4...",
    "file": "recadrage_lot_130_20261012.json",
    "links_count": 1,
    "size_bytes": 1690
  }
]
```

---

### 2. Format du Détail complet d'une mission
> **URL :** `GET https://webhook.kidpixel.fr/email_proxy.php?id=recadrage_lot_136`  
> *(ou avec l'ID complet : `?id=recadrage_lot_136_20261110`)*

Ce point d'entrée renvoie l'objet JSON unique du lot ciblé, avec le corps brut HTML et texte, les liens de livraison, les deadlines et les métadonnées complètes :

```json
{
  "schema_version": 1,
  "id": "recadrage_lot_136_20261110",
  "lot": 136,
  "subject": "Média Solution - Missions Recadrage - Lot 136",
  "received_at": "2026-11-10T12:56:17+01:00",
  "stored_at": "2026-11-11T20:30:14+01:00",
  "deadline": "13h45",
  "urgent": false,
  "sender_masked": "t***@media-solution.fr",
  "snippet": "Voici les missions pour le lot 136. Recadrage 9:16 standard à faire pour 13h45 (aujourd'hui 10/11) : - M6+ - THE 100 – Extrait 7 – Lexa Promesse.mp4 (Logos requis: m6+.png, the 100.png, m6+.mov)...",
  "body_html": "<div style=\"font-family: Arial, Helvetica, sans-serif; font-size: 14px; line-height: 1.45; color: #222222;\">Voici les missions pour le lot 136.<br><br><br>Recadrage 9:16 standard à faire pour 13h45 (aujourd&#x27;hui 10/11) :<br>- M6+ - THE 100 – Extrait 7 – Lexa Promesse.mp4 (Logos requis: m6+.png, the 100.png, m6+.mov)<br>- M6+ - THE 100 – Extrait 8 – Bellamy croit que Clarke est morte.mp4 (Logos requis: m6+.png, the 100.png, m6+.mov)<br><br>Lien : <a href=\"https://www.dropbox.com/scl/fo/itz9u2m2qxsua1inm33q7/AOruG3T8EGQg9WUOYmqKSAQ?rlkey=q48ikfhqhim8wp4vh3oawttrx&amp;dl=0\">https://www.dropbox.com/scl/fo/itz9u2m2qxsua1inm33q7/AOruG3T8EGQg9WUOYmqKSAQ?rlkey=q48ikfhqhim8wp4vh3oawttrx&amp;dl=0</a><br><br>Merci !<br></div>",
  "body_text": "Voici les missions pour le lot 136.\n\n\nRecadrage 9:16 standard à faire pour 13h45 (aujourd'hui 10/11) :\n- M6+ - THE 100 – Extrait 7 – Lexa Promesse.mp4 (Logos requis: m6+.png, the 100.png, m6+.mov)\n- M6+ - THE 100 – Extrait 8 – Bellamy croit que Clarke est morte.mp4 (Logos requis: m6+.png, the 100.png, m6+.mov)\n\nLien : https://www.dropbox.com/scl/fo/itz9u2m2qxsua1inm33q7/AOruG3T8EGQg9WUOYmqKSAQ?rlkey=q48ikfhqhim8wp4vh3oawttrx&dl=0\n\nMerci !",
  "body_omitted": false,
  "body_size_bytes": 845,
  "delivery_links": [
    {
      "provider": "dropbox",
      "raw_url": "https://www.dropbox.com/scl/fo/itz9u2m2qxsua1inm33q7/AOruG3T8EGQg9WUOYmqKSAQ?rlkey=q48ikfhqhim8wp4vh3oawttrx&dl=0",
      "direct_url": "https://www.dropbox.com/scl/fo/itz9u2m2qxsua1inm33q7/AOruG3T8EGQg9WUOYmqKSAQ?rlkey=q48ikfhqhim8wp4vh3oawttrx&dl=0",
      "r2_url": null,
      "original_filename": null
    }
  ],
  "email_id_hash": "sha256:6a21ffa256d412fa5e66cb11b42df7119ea7481f582babcb3e96914f120d6ab4",
  "source": "gmail_push"
}
```

---

### 3. Exemple sur un Lot Urgent (Lot 80)
> **URL :** `GET https://webhook.kidpixel.fr/email_proxy.php?id=recadrage_lot_80`

Pour une mission urgente, `"urgent": true` est activé, et les deux blocs d'instructions sont préservés :

```json
{
  "schema_version": 1,
  "id": "recadrage_lot_80_20261106",
  "lot": 80,
  "subject": "Média Solution - Missions Recadrage - Lot 80 - URGENT",
  "received_at": "2026-11-06T10:13:22+01:00",
  "stored_at": "2026-11-11T20:30:10+01:00",
  "deadline": "13h25",
  "urgent": true,
  "sender_masked": "t***@media-solution.fr",
  "snippet": "Voici les missions pour le lot 80. URGENT Traiter IMMÉDIATEMENT, à rendre au plus tard 11h00 (aujourd'hui 06/11) : - Leur belle-mère leur pourrit la vie !.mp4...",
  "body_html": "<div style=\"font-family: Arial, Helvetica, sans-serif; font-size: 14px; line-height: 1.45; color: #222222;\">Voici les missions pour le lot 80.<br><br><br>URGENT <br>Traiter IMMÉDIATEMENT, à rendre au plus tard 11h00 (aujourd&#x27;hui 06/11) :<br>- Leur belle-mère leur pourrit la vie !.mp4 (Pas de logo)<br><br><br><br>Recadrage 9:16 standard à faire pour 13h25 (aujourd&#x27;hui 06/11) :<br>- Gulli - MSL1 - L’interview « Quand j’étais petit » d’Igor Paixão.mp4 (Logos requis: MSL1.mov)<br><br>Lien : <a href=\"https://www.dropbox.com/scl/fo/tdnazmxmvzrqaala2c0eu/AAYzC23am01ntYvZc-9BZ-8?rlkey=oys6dd380eruesbppb7wcpz0m&amp;dl=0\">https://www.dropbox.com/scl/fo/tdnazmxmvzrqaala2c0eu/AAYzC23am01ntYvZc-9BZ-8?rlkey=oys6dd380eruesbppb7wcpz0m&amp;dl=0</a><br><br>Merci !<br></div>",
  "body_text": "Voici les missions pour le lot 80.\n\n\nURGENT \nTraiter IMMÉDIATEMENT, à rendre au plus tard 11h00 (aujourd'hui 06/11) :\n- Leur belle-mère leur pourrit la vie !.mp4 (Pas de logo)\n\n\n\nRecadrage 9:16 standard à faire pour 13h25 (aujourd'hui 06/11) :\n- Gulli - MSL1 - L’interview « Quand j’étais petit » d’Igor Paixão.mp4 (Logos requis: MSL1.mov)\n\nLien : https://www.dropbox.com/scl/fo/tdnazmxmvzrqaala2c0eu/AAYzC23am01ntYvZc-9BZ-8?rlkey=oys6dd380eruesbppb7wcpz0m&dl=0\n\nMerci !",
  "body_omitted": false,
  "body_size_bytes": 1056,
  "delivery_links": [
    {
      "provider": "dropbox",
      "raw_url": "https://www.dropbox.com/scl/fo/tdnazmxmvzrqaala2c0eu/AAYzC23am01ntYvZc-9BZ-8?rlkey=oys6dd380eruesbppb7wcpz0m&dl=0",
      "direct_url": "https://www.dropbox.com/scl/fo/tdnazmxmvzrqaala2c0eu/AAYzC23am01ntYvZc-9BZ-8?rlkey=oys6dd380eruesbppb7wcpz0m&dl=0",
      "r2_url": null,
      "original_filename": null
    }
  ],
  "email_id_hash": "sha256:d8a9f4e2c9...",
  "source": "gmail_push"
}
```

---

### 4. Synthèse des types et clés pour votre intégration

| Clé | Type | Description |
|---|---|---|
| `lot` | `integer` | Numéro de lot extrait (ex: `136`) |
| `urgent` | `boolean` | `true` si le sujet contient `URGENT` ou `URGENCE` |
| `deadline` | `string` | Heure limite détectée (ex: `"13h45"`, `"le 10/11/2026 à 18h00"`) |
| `body_text` | `string` | Texte brut assaini, idéal pour parser les lignes de vidéos |
| `body_html` | `string` | Rendu HTML original de l'email |
| `delivery_links[].raw_url` | `string` | Lien Dropbox original présent dans l'email |
| `received_at` | `string (ISO 8601)` | Horodatage de réception du message |
