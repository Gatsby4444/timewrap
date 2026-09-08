# Timewrap

Application Android d'emploi du temps, pensée pour répondre en une seconde à la seule question
qui compte quand on sort d'un cours : *je suis où maintenant, il me reste combien de temps, c'est
quoi après ?*

Elle lit les exports **iCalendar** d'un ENT — fichier `.ics` ou URL d'abonnement — et les
restitue dans une interface mobile, hors-ligne, sans compte ni serveur.

> État : **phase 0**. Le squelette et la chaîne de compilation sont en place ; l'application
> n'affiche pour l'instant qu'un auto-diagnostic de sa pile native. Voir la feuille de route.

## Architecture

Deux moitiés, une frontière nette.

| | |
|---|---|
| **`core/`** — Rust | Tout le domaine métier : parsing iCalendar, expansion des récurrences (RRULE/RDATE/EXDATE), fuseaux et heure d'été, stockage SQLite, moteur de règles de renommage, diff de synchronisation, calcul des rappels. |
| **`android/`** — Kotlin + Jetpack Compose | Uniquement ce qui doit être natif : rendu, gestes, réseau, alarmes, notifications, sélecteur de fichiers. |

Le pont entre les deux est généré par [UniFFI](https://mozilla.github.io/uniffi-rs/) : Gradle
compile la crate pour chaque ABI Android avec `cargo-ndk`, puis dérive les bindings Kotlin
directement depuis la bibliothèque produite. Aucune interface n'est écrite à la main des deux
côtés, donc aucune ne peut diverger.

Ce découpage sert aussi le portage iOS prévu plus tard : UniFFI génère également du Swift, et
`core/` sera réutilisé tel quel.

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
| **1** | Parsing iCalendar, stockage, import de fichier, vues Maintenant / Jour / Semaine |
| **2** | Abonnement par URL, synchronisation automatique, notifications de changement, rappels |
| **3** | Renommage, couleurs et masquage des cours, avec suggestions automatiques |
| **4** | Agendas locaux éditables, vues Cours / Perso / Projets, notes et devoirs |
| **5** | Widget, finitions, puis portage iOS sur le même cœur |

## Licence

MIT — voir [LICENSE](LICENSE).
