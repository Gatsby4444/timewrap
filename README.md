# Timewrap

Application Android d'emploi du temps, pensée pour répondre en une seconde à la seule question
qui compte quand on sort d'un cours : *je suis où maintenant, il me reste combien de temps, c'est
quoi après ?*

Elle lit les exports **iCalendar** d'un ENT — fichier `.ics` ou URL d'abonnement — et les
restitue dans une interface mobile, hors-ligne, sans compte ni serveur.

> État : **phases 1, 3 et 4** livrées. L'application lit un export `.ics`, le stocke hors ligne
> et l'affiche dans trois vues — Maintenant, Jour, Semaine. Chaque agenda est un dossier que l'on
> peut ouvrir seul, on y saisit ses propres créneaux, un moteur central signale les
> chevauchements, et des règles visuelles colorient l'emploi du temps par type de cours. Reste la
> phase 2 : abonnement par URL, synchronisation automatique et rappels. Voir la feuille de route.

## Architecture

Deux moitiés, une frontière nette.

| | |
|---|---|
| **`core/`** — Rust | Tout le domaine métier : parsing iCalendar, expansion des récurrences (RRULE/RDATE/EXDATE), fuseaux et heure d'été, stockage SQLite, moteur de règles visuelles, moteur de chevauchements, diff de synchronisation et calcul des rappels à venir. |
| **`android/`** — Kotlin + Jetpack Compose | Uniquement ce qui doit être natif : rendu, gestes, réseau, alarmes, notifications, sélecteur de fichiers. |

Le pont entre les deux est généré par [UniFFI](https://mozilla.github.io/uniffi-rs/) : Gradle
compile la crate pour chaque ABI Android avec `cargo-ndk`, puis dérive les bindings Kotlin
directement depuis la bibliothèque produite. Aucune interface n'est écrite à la main des deux
côtés, donc aucune ne peut diverger.

Ce découpage sert aussi le portage iOS prévu plus tard : UniFFI génère également du Swift, et
`core/` sera réutilisé tel quel.

## Les trois moteurs

Trois questions reviennent sans cesse quand on tient plusieurs emplois du temps. Chacune a son
module dans `core/`, sans état ni base de données, donc testable ligne à ligne.

### Les agendas sont des dossiers

Un agenda — un export d'ENT, un agenda personnel créé sur place — s'ouvre **seul** ou se mêle aux
autres. Toutes les vues acceptent une *portée* : absente, elles montrent les agendas non masqués ;
renseignée, elle l'emporte sur la visibilité, parce qu'ouvrir un dossier doit le montrer même
décoché dans la vue d'ensemble. L'écran d'accueil en donne une tuile par agenda : ce qui vient
cette semaine, la prochaine séance, les chevauchements en attente.

### Le moteur de chevauchements — `core/src/conflict.rs`

Toute écriture d'événement passe par `save_event`, et **rien n'est écrit tant qu'un chevauchement
subsiste** : le cœur rend la liste des heurts, l'interface pose la question, puis rappelle avec la
décision. Quatre issues :

| | |
|---|---|
| **Annuler** | On renonce ; c'est le défaut, celui qui déclenche la question. |
| **Remplacer** | Fait place nette : supprime les créneaux saisis ici, masque les séances importées — les effacer serait vain, le prochain import les ramènerait. |
| **Décaler après** | Repousse le nouveau créneau juste après le dernier conflit, durée conservée, en cascade s'il en heurte un autre. |
| **Ignorer** | Assume le chevauchement : deux cours peuvent légitimement se superposer. |

Le heurt est qualifié — même agenda ou agendas différents — et chiffré en minutes de recouvrement.
Deux créneaux qui s'enchaînent ne comptent pas : finir à 10:00 et commencer à 10:00 est un
enchaînement, pas un conflit. Un gestionnaire liste par ailleurs les chevauchements déjà en place
sur les deux mois qui viennent, et rappelle les séances masquées pour que « Remplacer » reste
réversible.

### Le moteur de règles visuelles — `core/src/rules.rs`

Un ENT livre « R3.01 DEV WEB - CM (Gr A) » et rien qui distingue un amphi d'un TP. Plutôt que de
recolorier séance par séance, on décrit **une fois** ce qui les reconnaît : un champ (titre, lieu,
notes, ou les trois), une comparaison (mot entier, contient, commence par…), et trois effets
cumulables — poser une catégorie donc une couleur, renommer, masquer.

Le cœur propose d'abord : il repère dans les intitulés importés les mots qui reviennent dans
plusieurs séances mais pas dans toutes — ceux-là découpent l'emploi du temps — et met les
marqueurs de type connus (CM, TD, TP, examen…) devant les noms de matière. Accepter une suggestion
crée la catégorie et la règle d'un geste.

Les décisions sont matérialisées à l'écriture, jamais recalculées à l'affichage, et rejouées d'un
bloc quand une règle change. L'intitulé de l'ENT est conservé à côté du titre affiché : retirer
une règle rend son nom d'origine à la séance, sans réimport. Une catégorie posée à la main sur une
séance précise résiste aux règles — l'exception survit au moteur.

## Prérequis

- **Rust** stable (édition 2024) et les cibles Android :
  ```sh
  rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
  cargo install cargo-ndk
  ```
- **JDK 17**
- **SDK Android** avec la plateforme 36 et un **NDK** (`ANDROID_HOME` doit être défini)

### Windows : toolchain MinGW requise

La toolchain Rust `x86_64-pc-windows-gnu` livrée par rustup ne contient pas l'assembleur `as.exe`
dont dépend `dlltool`. Sans lui, aucun outil hôte (`cargo-ndk`, `uniffi-bindgen`, `cargo test`)
ne compile. Installer une fois :

```powershell
winget install --id BrechtSanders.WinLibs.POSIX.MSVCRT --exact
```

La variante *MSVCRT* est choisie pour correspondre à la cible `windows-gnu` de Rust, ce qui
compte dès qu'un crate compile du C — le SQLite embarqué, notamment.

## Compiler

```sh
# Tests du cœur, sur la machine de développement
cargo test

# APK de debug (compile le Rust, génère les bindings, assemble)
cd android && ./gradlew assembleDebug

# APK de release
cd android && ./gradlew assembleRelease
```

L'APK atterrit dans `android/app/build/outputs/apk/`.

## Signature et livraison

Les APK publiés sont signés avec une clé permanente : les mises à jour s'installent par-dessus la
version précédente sans désinstallation. La clé n'est **jamais** versionnée.

- **En local** : `android/keystore.properties` (ignoré par git) avec `storeFile`, `storePassword`,
  `keyAlias`, `keyPassword`.
- **En CI** : les secrets GitHub `TIMEWRAP_KEYSTORE_BASE64`, `TIMEWRAP_STORE_PASSWORD`,
  `TIMEWRAP_KEY_ALIAS`, `TIMEWRAP_KEY_PASSWORD`.

Sans clé disponible, Gradle retombe sur la signature debug pour que le projet reste compilable
par n'importe qui.

Pousser un tag `v*` déclenche la construction et publie l'APK en release GitHub :

```sh
git tag v0.1.0 && git push origin v0.1.0
```

## Données personnelles

Le dépôt est public. `.gitignore` exclut `samples/local/`, tout `.ics` hors fixtures de test, et
tout fichier de clé. Un emploi du temps réel ou une URL d'abonnement d'ENT ne doivent jamais y
entrer : les fixtures de `core/tests/fixtures/` sont anonymisées.

## Feuille de route

| Phase | Contenu |
|---|---|
| **0** ✅ | Squelette, chaîne Rust → NDK → APK signé, CI, auto-diagnostic embarqué |
| **1** ✅ | Lecture iCalendar, stockage, import de fichier, vues Maintenant / Jour / Semaine |
| **3** ✅ | Catégories, couleurs et masquage par règle, avec suggestions déduites des imports |
| **4** ✅ | Agendas locaux éditables, écran d'accueil par dossier, moteur de chevauchements |
| **2** | Abonnement par URL, synchronisation automatique, notifications de changement, rappels |
| **5** | Notes et devoirs, widget, finitions, puis portage iOS sur le même cœur |

## Licence

MIT — voir [LICENSE](LICENSE).
