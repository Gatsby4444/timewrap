# Timewrap

Application Android d'emploi du temps, pensée pour répondre en une seconde à la seule question
qui compte quand on sort d'un cours : *je suis où maintenant, il me reste combien de temps, c'est
quoi après ?*

Elle lit les exports **iCalendar** d'un ENT — fichier `.ics` ou URL d'abonnement — et les
restitue dans une interface mobile, hors-ligne, sans compte ni serveur.

> État : **phases 1 à 4** livrées. L'emploi du temps se charge par fichier ou par abonnement, se
> resynchronise tout seul et prévient de ce qui change. Les couleurs se règlent par champ de
> l'export — une par type de cours, une par matière. Une liste de choses à faire, sans heure,
> accompagne chaque journée et se reporte tant qu'elle n'est pas cochée. Rappels avant les cours
> et résumé du matin compris.

## Architecture

Deux moitiés, une frontière nette.

| | |
|---|---|
| **`core/`** — Rust | Tout le domaine métier : parsing iCalendar, expansion des récurrences (RRULE/RDATE/EXDATE), fuseaux et heure d'été, stockage SQLite, lecture des champs structurés, règles visuelles, moteur de chevauchements, différence entre deux synchronisations, choses à faire, calcul des rappels. |
| **`android/`** — Kotlin + Jetpack Compose | Uniquement ce qui doit être natif : rendu, gestes, réseau, alarmes, notifications, sélecteur de fichiers. |

Le pont entre les deux est généré par [UniFFI](https://mozilla.github.io/uniffi-rs/) : Gradle
compile la crate pour chaque ABI Android avec `cargo-ndk`, puis dérive les bindings Kotlin
directement depuis la bibliothèque produite. Aucune interface n'est écrite à la main des deux
côtés, donc aucune ne peut diverger.

La règle d'architecture, tenue partout : **aucune décision métier ne se prend au-dessus de cette
frontière**. L'interface affiche et demande ; elle ne recalcule jamais une couleur, ni un conflit,
ni l'instant d'un rappel. Même les phrases qui annoncent un changement d'emploi du temps sont
rédigées par le cœur, parce qu'elles dépendent du fuseau et de ce qui a bougé.

Ce découpage sert aussi le portage iOS prévu plus tard : UniFFI génère également du Swift, et
`core/` sera réutilisé tel quel.

## Ce que fait l'application

### Un emploi du temps, pas une collection d'agendas

Il n'y en a qu'un. Réimporter un fichier ou resynchroniser une URL en remplace le contenu sans
changer son identité : les créneaux ajoutés à la main, les séances masquées et les couleurs
survivent. Ce qui n'a pas d'heure — un devoir, une démarche — n'est pas un créneau et n'a donc
rien à faire dans un second agenda : c'est une tâche, et elle a sa liste.

### Les couleurs viennent des champs de l'export

Un ENT range l'essentiel dans la description, en clair et par lignes :

```
Intervenant : MARTIN Camille
Type : Travaux dirigés
Matière : Analyse
Salle : C204
```

Le cœur lit ces couples clé/valeur à l'import, normalise les clés — « Matière », « matiere » et
« MATIÈRE » désignent le même champ —, et l'application en fait la matière de son écran des
couleurs : *Colorier par Type*, *Colorier par Matière*, une ligne par valeur, une pastille à
changer. « Tout colorier » distribue la palette d'un coup, et il ne reste qu'à corriger ce qui ne
plaît pas.

Sous le capot, chaque couleur est une catégorie et la règle qui la pose — mais l'utilisateur n'a
vu qu'une pastille. Ces règles s'appliquent aussi à ce qui arrive **ensuite** : un nouveau TD
importé le mois prochain prend sa couleur sans rien redemander.

Pour ce que les champs ne couvrent pas, les **règles avancées** restent accessibles : reconnaître
un mot dans l'intitulé, renommer (« ★ {} » préfixe sans retaper), masquer. Elles se cumulent dans
l'ordre de priorité, et chacune n'écrase que ce qu'elle renseigne. L'intitulé de l'ENT est
conservé à côté du titre affiché : retirer une règle rend son nom d'origine à la séance, sans
réimport.

### La liste de choses à faire

Sans heure — ce n'est pas un créneau, c'est une intention. Non cochée le soir, une tâche reparaît
le lendemain **avec son retard affiché** : « Prévu hier, pas fait », « En retard de 3 jours ». Le
report n'est pas un travail de fond qui déplace des lignes, c'est une conséquence de la requête :
tant qu'une tâche n'est pas cochée, elle reste due.

Une journée passée ou à venir ne montre que ce qui lui était propre — sans quoi la vue Semaine
répéterait sept fois la même liste.

### Le moteur de chevauchements

Toute écriture d'événement passe par un point unique, et **rien n'est écrit tant qu'un
chevauchement subsiste** : le cœur rend la liste des heurts, l'interface pose la question, puis
rappelle avec la décision.

| | |
|---|---|
| **Annuler** | On renonce ; c'est le défaut, celui qui déclenche la question. |
| **Remplacer** | Fait place nette : supprime les créneaux saisis ici, masque les séances importées — les effacer serait vain, le prochain import les ramènerait. |
| **Décaler après** | Repousse le nouveau créneau juste après le dernier conflit, durée conservée, en cascade s'il en heurte un autre. |
| **Ignorer** | Assume le chevauchement : deux créneaux peuvent légitimement se superposer. |

Deux créneaux qui s'enchaînent ne comptent pas : finir à 10:00 et commencer à 10:00 est un
enchaînement, pas un conflit. Un gestionnaire liste par ailleurs les chevauchements déjà en place,
et rappelle les séances masquées pour que « Remplacer » reste réversible.

### Synchronisation et notifications

L'abonnement par URL — `https://` comme `webcal://` — se retélécharge tout seul à l'intervalle
choisi, sous contrainte de réseau, par WorkManager. Après chaque synchronisation, le cœur compare
l'avant et l'après **en appariant les séances par UID** : une séance qui garde le sien a été
déplacée, pas supprimée puis recréée. C'est la différence entre « ton TD d'Analyse passe du mardi
13:30 au mardi 15:00 » et deux notifications illisibles.

Trois canaux de notification distincts, parce que trois urgences distinctes : les changements, les
rappels avant les cours (délai réglable), le résumé du matin. Les séparer laisse couper l'un sans
perdre l'autre, ce que le réglage système fait très bien à notre place. Les alarmes sont exactes
quand Android l'autorise, approchées sinon, et reposées après un redémarrage.

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

**Attention à l'ordre du `PATH`** : rustup installe un `x86_64-w64-mingw32-gcc` incomplet qui
passe devant celui de WinLibs et fait échouer l'édition de liens (`cannot find crt2.o`). Mettre
WinLibs en tête avant de lancer les tests :

```sh
export PATH="$LOCALAPPDATA/Microsoft/WinGet/Packages/BrechtSanders.WinLibs.POSIX.MSVCRT_Microsoft.Winget.Source_8wekyb3d8bbwe/mingw64/bin:$PATH"
```

`cargo check` n'est pas concerné — il ne lie pas —, et Gradle non plus, qui compile pour les ABI
Android via `cargo-ndk`.

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
git tag v0.4.0 && git push origin v0.4.0
```

## Données personnelles

Le dépôt est public. `.gitignore` exclut `samples/local/`, tout `.ics` hors fixtures de test, et
tout fichier de clé. Un emploi du temps réel ou une URL d'abonnement d'ENT ne doivent jamais y
entrer : les fixtures de `core/tests/fixtures/` sont anonymisées.

Sur l'appareil, la base vit dans le stockage privé de l'application. Le réseau ne sert qu'à
retélécharger l'adresse d'abonnement fournie par l'utilisateur ; rien d'autre ne sort.

## Feuille de route

| Phase | Contenu |
|---|---|
| **0** ✅ | Squelette, chaîne Rust → NDK → APK signé, CI, auto-diagnostic embarqué |
| **1** ✅ | Lecture iCalendar, stockage, import de fichier, vues Maintenant / Jour / Semaine |
| **2** ✅ | Abonnement par URL, synchronisation automatique, notifications de changement, rappels |
| **3** ✅ | Couleurs par champ de l'export, règles visuelles, renommage et masquage |
| **4** ✅ | Événements saisis sur place, moteur de chevauchements, liste de choses à faire reportable |
| **5** | Devoirs et notes rattachés à un cours, widget, finitions, puis portage iOS sur le même cœur |

## Licence

MIT — voir [LICENSE](LICENSE).
