# Rappels

La mémoire longue du projet : ce qui attend une **décision** de Jay, ou une limite connue à
ressortir au bon moment. Ne rien supprimer sans accord de Jay.

## 1. Palette de couleurs en double — 2026-11-08 — ✅ réglé le 2026-11-09 (v0.7.0)

La palette existe deux fois : dans le cœur (`core/src/store.rs`, `PALETTE`, 8 couleurs) et
recopiée dans l'interface (`android/…/ui/Common.kt`). Rien ne garantit qu'elles restent égales.
À régler avec le chantier « plus de couleurs » : le cœur sert la palette, l'interface l'affiche.

## 2. Vérification sur appareil des correctifs v0.6.0 — 2026-11-08

Le décalage entre l'en-tête et la grille (Jour / Semaine) et la troncature des notifications ne
se reproduisent pas par un test du cœur : ce sont des comportements d'écran. Ils ont été corrigés
par la cause (voir le rapport de session du 2026-11-08), mais la confirmation vient du téléphone.
À vérifier par Jay : bouton « Aujourd'hui » / « Cette semaine » après avoir glissé loin ; appli
laissée ouverte la nuit puis rouverte le matin ; notification de plusieurs changements dépliée.

## 3. Normalisation des noms de champs — 2026-11-09

Depuis la v0.7.0, une majuscule accentuée est mise en minuscule (« É » → « e ») dans les noms de
champs. Un champ dont le nom **commence** par une majuscule accentuée (« Échéance : … ») change donc
de clé à la prochaine synchronisation : une couleur posée sur ses valeurs avant la v0.7.0 serait à
reposer. Les champs usuels (Type, Matière, Salle, Intervenant) ne sont pas concernés.
