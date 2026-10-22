# Panel EffectCraft

`MediaSolution.jsx` est la copie du panel de production
`workflow_mediapipe/scripts/MediaSolution-EC/MediaSolution.jsx`, utilisée par
STEP8 pour construire le driver headless.

- Source : dépôt `workflow_mediapipe`, version `12.0-ec`.
- Copie : 2026-10-22.
- Le panel n'est pas modifié par le wrapper ; le driver est concaténé à
  l'exécution.
- `mediasolution_ec.json` est un instantané de la configuration de production
  (`/mnt/cache/mediasolution_ec.json`, 2026-10-22). STEP8 peut le remplacer par
  `--config-json` ou par `steps.step8.panel_config`.
