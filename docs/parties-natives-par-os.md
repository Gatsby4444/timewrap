# Parties natives, par système

Tout ce qui ne vit pas dans `core/` (Rust, partagé) et devra donc être réécrit en Swift pour iOS.
Mis à jour à chaque fichier Kotlin ajouté, supprimé ou renommé, et vérifié en fin de session
(règle du `CLAUDE.md`). Dernière vérification : 2026-11-08, v0.6.0.

## Capacités système

| Capacité | Android (fait) | iOS (à écrire) |
|---|---|---|
| Ouvrir la base, fuseau de l'appareil | `TimewrapApp.kt` — `Timewrap.open(dbPath, ZoneId.systemDefault())` | `TimeZone.current.identifier`, base dans `Application Support` |
| Téléchargement de l'abonnement | `sync/IcsFetcher.kt` — HTTPS, `webcal://` → `https://`, échecs traduits en français | `URLSession` ; même traduction des erreurs |
| Racines GÉANT/HARICA ajoutées au magasin | `res/raw/geant_roots.pem` + `res/xml/network_security_config.xml` | À vérifier : iOS récents connaissent HARICA 2021 ; sinon épinglage via `URLSessionDelegate` |
| Synchronisation périodique | `sync/SyncWorker.kt` — WorkManager, réseau requis, intervalle réglable | `BGAppRefreshTask` (aucune garantie d'horaire sur iOS) |
| Notifications de changement | `notify/Notifications.kt` — une par changement, groupées sous un résumé, ouvrent l'historique (extra `app.timewrap.open = history`) | `UNUserNotificationCenter`, `threadIdentifier` pour grouper, action d'ouverture vers l'historique |
| Rappels avant les cours | `notify/ReminderScheduler.kt` + `notify/ReminderReceiver.kt` — alarmes exactes si permises | `UNCalendarNotificationTrigger` (limite de 64 notifications en attente) |
| Résumé du matin | `notify/ReminderReceiver.kt` | idem, déclencheur quotidien |
| Reposer les alarmes au redémarrage | `notify/BootReceiver.kt` | inutile : iOS conserve les notifications programmées |
| Ouvrir / partager un `.ics` | `MainActivity.kt` — `ACTION_VIEW`, `ACTION_SEND`, sélecteur de documents | `UIDocumentPickerViewController`, type UTI `public.calendar-event` |

## Interface

Tout `android/app/src/main/java/app/timewrap/ui/` et `AppViewModel.kt` est de la présentation
Compose : à refaire en SwiftUI, sans aucune règle métier à réécrire (elles sont dans le cœur).

| Écran | Fichier |
|---|---|
| Maintenant | `ui/NowScreen.kt` |
| Jour, Semaine (pagers à origine fixe) | `ui/DayWeekScreens.kt`, `ui/TimeGrid.kt` |
| À faire | `ui/TasksScreen.kt` |
| Réglages | `ui/SettingsScreen.kt` |
| Historique des changements | `ui/HistoryScreen.kt` |
| Couleurs, règles | `ui/ColorsScreen.kt`, `ui/RulesScreen.kt` |
| Chevauchements | `ui/ConflictsScreen.kt` |
| Éditeur d'événement | `ui/EventEditor.kt` |
| Composants partagés, formats | `ui/Common.kt`, `ui/Format.kt`, `ui/theme/Theme.kt` |

⚠️ Dette connue : la palette de couleurs est recopiée dans `ui/Common.kt` alors que le cœur a
la sienne (`core/src/store.rs`, `PALETTE`). Deux copies d'une même donnée : à faire servir par le
cœur, sans quoi iOS en ferait une troisième.
