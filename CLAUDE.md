# Timewrap — Contexte projet pour Claude Code

Application Android d'emploi du temps, lue depuis les exports iCalendar d'un ENT (fichier `.ics`
ou URL d'abonnement). Hors-ligne, sans compte ni serveur. Le `README.md` décrit le produit,
l'architecture, la compilation et la livraison : le lire en début de session.

> Les règles de méthode ci-dessous sont reprises le **2026-11-08** du `CLAUDE.md` de NeoVibe
> Alpha (`D:\projets\neovibe_alpha\CLAUDE.md`), à la demande de Jay. Seules les règles valables
> pour n'importe quel projet ont été gardées, adaptées à l'architecture de Timewrap. Tout ce qui
> était propre à NeoVibe (vision produit, Supabase, serveur Rust, BLE, sous-agents) est resté là-bas.

## Architecture en une phrase

**`core/` (Rust) décide, `android/` (Kotlin + Compose) affiche.** Le pont est généré par UniFFI.
Aucune décision métier — couleur, conflit, instant d'un rappel, texte d'un changement, date
affichée — ne se prend au-dessus de cette frontière. Le cœur sera réutilisé tel quel pour iOS.

---

## Règle impérative : UN FAIT SE VÉRIFIE À LA SOURCE, JAMAIS DANS UN DOCUMENT

> « Tu ne dois pas te fier aux rapports d'anciennes sessions, ni aux vieux documents explicatifs,
> ni aux commentaires : il faut **vérifier dans le code** (ou sur l'artefact) pour contrôler une
> information avant d'en déduire quoi que ce soit. »

| Question | Où la vérifier |
|---|---|
| Ce que fait le code | **dans le code**, en déroulant la chaîne jusqu'au bout (de l'écran Compose jusqu'au cœur Rust) |
| Ce que produit un build | **sur l'artefact** (APK, manifeste fusionné, `aapt`, `apksigner`) |
| Le comportement d'une règle du cœur | **par un test** qui la rejoue |
| Ce qui s'est passé sur l'appareil | **dans ce que Jay a observé pendant CE test** |

Ne comptent **pas** comme source : un rapport de session, `RAPPELS.md`, un commentaire, le
`README.md`, ma propre mémoire. Ils disent quoi aller vérifier, jamais quoi conclure. **Un document
périmé est indiscernable d'un document juste.**

Corollaire : quand une conclusion repose sur un fait non vérifié, le dire — « je suppose X, à
confirmer » — au lieu de l'énoncer comme acquis.

---

## Règle impérative : LA FIABILITÉ VIENT DE L'ARCHITECTURE, PAS DU COLMATAGE

> « On doit construire une architecture robuste et claire, ne pas tout mélanger et colmater les
> brèches. La fiabilité vient de l'architecture et de la structure, pas du rebouchage des failles.
> Et **on sépare clairement et distinctement tout ce que l'on peut**. »

1. **Devant un risque, supprimer d'abord la cause**, ensuite seulement empêcher qu'elle nuise. Un
   garde-fou est un aveu ; une cause supprimée ne coûte plus rien.
2. **Deux objets qui n'obéissent pas aux mêmes règles ne partagent ni la même table, ni le même
   chemin d'accès.** Sinon la règle la plus permissive gagne, en silence.
3. **Compter les chemins qui mènent à une donnée.** Plusieurs chemins aux règles différentes =
   défaut de conception.
4. **Une règle s'énonce positivement.** « Ce cas n'existe pas » vaut mieux que « ce cas existe mais
   rien ne le déclenche ».
5. **Après un changement d'architecture, rejouer les décisions qui en dépendaient** au lieu de
   dérouler un plan écrit avant.
6. **Ne jamais livrer un correctif fondé sur une déduction.** Reproduire la panne d'abord — de
   préférence par un test du cœur qui échoue avant la correction et passe après. Une hypothèse
   plausible qui ne corrige rien fait perdre un aller-retour de test à Jay.
7. **Toute suppression est une opération sur un réseau — relever les DEUX sens avant de couper.**
   - *Qui m'appelle ?* Balayer toutes les familles : Rust, interface UniFFI, Kotlin, manifeste,
     ressources, WorkManager, alarmes, receivers. Un appelant oublié ne se voit pas au diff.
   - *Qu'est-ce que j'appelais ?* Ce qui devient orphelin se supprime dans la foulée, ou se justifie.
   - Vérifier par inventaire (`grep` du motif supprimé), pas par le diff.

---

## Règle impérative : DISSOCIER L'ACQUISITION DE L'USAGE — ON SÉPARE TOUT CE QUI PEUT L'ÊTRE

> « Il faut totalement dissocier le code qui se charge d'acquérir des données du code qui utilise
> ces données. Il faut un code compartimenté et localement indépendant. »

| Rôle | Ici | Ce qu'il ne fait JAMAIS |
|---|---|---|
| **La cuisine** (acquisition) | `sync/` (réseau), `store.rs` (disque), `ics.rs` (parsing) | décider de ce qui s'affiche |
| **Le serveur** (règles, vues dérivées) | le cœur Rust, `AppViewModel` qui relaie | aller cuisiner lui-même |
| **Le client** (écrans) | `ui/*.kt` | parler au réseau ou au disque, recalculer une règle |

1. **Un écran ne parle jamais au réseau, au disque ou au système.** Il demande au ViewModel, qui
   demande au cœur.
2. **Un chemin, une donnée.** Deux chemins vers la même chose, c'est deux caches et un désaccord
   futur que rien ne signalera.
3. **Le temps est une SOURCE, pas une commodité.** Tout calcul qui appelle « maintenant » ou
   « aujourd'hui » dépend d'une donnée qu'il n'observe pas : on la passe en paramètre (le cœur
   reçoit l'instant et le fuseau), on ne la relit pas en douce au milieu d'un calcul.
4. **Un état, une seule source de vérité.** Une date affichée et la date des données affichées ne
   sont pas deux états à synchroniser : l'une se déduit de l'autre.
5. **Scalable veut dire : le coût d'ajouter le prochain cas.** Compter les endroits à modifier
   *avant* de choisir une solution.
6. **Ces défauts ne lèvent aucune erreur** : ils ne se voient qu'en comptant. Un test qui compte
   vaut mieux qu'un commentaire qui promet.

Le test à s'appliquer : *pour changer la présentation, dois-je toucher à ce qui prépare la
donnée ?* Si oui, la séparation n'est pas faite.

---

## Règle impérative : LA RÈGLE VIT DANS LE CŒUR, JAMAIS SEULEMENT DANS L'ÉCRAN

Équivalent, pour une application sans serveur, de « la règle vit au serveur » de NeoVibe. Ce qu'un
écran cache ou filtre n'est pas une règle : c'est une décision que le portage iOS devra réécrire,
et qui divergera.

1. **Avant d'écrire une règle dans le Kotlin, écrire où le cœur la tient.** Si la réponse est
   « nulle part », ce n'est pas encore une règle.
2. **Une règle qui doit valoir pour tous les chemins se pose là où tous passent** (la fonction du
   cœur), pas dans chaque écran un par un.
3. **Toute règle du cœur a son test** dans `core/src/tests.rs`.

---

## Règle impérative : S'INSPIRER DE CEUX QUI ONT DÉJÀ RÉSOLU

> « Au niveau technique on doit s'inspirer des autres, car ils ont déjà des systèmes aboutis. Avant
> de coder des points techniques, on se documente sur la manière de faire des autres et on voit
> ensemble quelle est la meilleure solution. »

1. **Chercher comment font les autres** (Google Agenda, Etar, Simple Calendar, Proton Calendar,
   les apps d'emploi du temps étudiantes…), par ordre de confiance : leur code source quand il est
   public ; leur documentation officielle ; ce que des gens ont mesuré. Dire d'où vient chaque fait.
2. **Comparer avec Timewrap**, en langage simple.
3. **Passer leur solution au filtre du projet** (hors-ligne, sans compte, règle dans le cœur).
4. **Présenter à Jay et choisir ensemble** pour un choix technique structurant.

Ce que la règle ne veut pas dire : copier sans comprendre ; retarder la réparation d'un défaut déjà
reproduit ; importer leurs choix produit, qui restent à Jay.

---

## Règle impérative : UN DÉFAUT TROUVÉ SE RÉPARE TOUT DE SUITE

> « On ne laisse jamais des défauts trouvés en attente. Après, c'est cela qui nous perd et qui crée
> de plus grosses erreurs. »

1. **Réparer dans la foulée**, dans la même session. Consigner ne remplace jamais réparer :
   `RAPPELS.md` sert à ce qui demande une **décision** de Jay.
2. **Le défaut qu'on vient de créer soi-même passe en premier.**
3. **Si la réparation ne peut vraiment pas se faire maintenant**, le dire à Jay avec la raison — et
   c'est Jay qui reporte.

Le cas le plus coûteux est un **instrument défaillant** (un libellé qui ment, une date affichée
fausse) : il fausse toutes les décisions prises ensuite sans que rien ne le signale.

Le test : *« est-ce que je viens d'écrire, ou de dire, que quelque chose ne va pas ? »* Si oui, ça
se répare maintenant.

---

## Règle impérative : RAPPORT DE SESSION

À chaque nouvelle session, créer un rapport dans `rapports-de-sessions/`, nommé
`AAAA-MM-JJ_HH-MM.md` (heure de début de session).

- **Document vivant** : mis à jour à chaque modification significative, pas seulement à la fin.
- **Contenu obligatoire** :
  1. **Modifications apportées** — fichiers touchés, fonctionnalités, décisions d'implémentation
  2. **Difficultés rencontrées ou erreurs commises** — cause identifiée, et comment ne pas les
     reproduire
  3. **Notes et consignes de Jay** — toute instruction ou préférence donnée pendant la session
- Avant de commencer, **relire les 1 à 3 derniers rapports**.
- Factuel et concis : un outil de mémoire technique, pas une présentation.

## Règle impérative : FICHIER DE RAPPELS

`RAPPELS.md` à la racine est la mémoire longue de Jay.

- Y consigner (sujet, détail, date) ce que Jay demande de garder pour plus tard, et toute limite
  ou dette connue qui attend une décision.
- Le ressortir au bon moment — avant une release, ou quand le chantier revient — sans attendre
  qu'on le demande.
- Ne rien y supprimer sans validation explicite de Jay. Le relire en début de session.

## Règle impérative : CATALOGUE DES PARTIES NATIVES

Le portage iOS est prévu sur le même cœur. Tout ce qui vit dans `android/` devra être réécrit en
Swift : le `README.md` (section Architecture) et ce catalogue doivent permettre de n'en rien
découvrir au dernier moment. À chaque fichier Kotlin ajouté, supprimé ou renommé, ou nouvelle
capacité système (notification, alarme, widget, réseau), **mettre à jour
`docs/parties-natives-par-os.md`** et le vérifier en fin de session.

---

## Conventions de code

- **Rust** : `cargo fmt --all` et `cargo clippy --all-targets -- -D warnings` propres, `cargo test`
  au vert. C'est ce que la CI exige.
- **Windows** : WinLibs doit passer devant rustup dans le `PATH` avant `cargo test` et `./gradlew`
  (voir `README.md`, section Prérequis).
- Structure et nommage : suivre l'existant, ne pas réorganiser sans consigne.
- Un commit = un changement logique. Messages en français, courts et descriptifs.
- Commentaires, messages de commit et rapports en français ; les identifiants restent en anglais.

## Sécurité et données personnelles

- **Ne jamais committer de clé, de mot de passe de keystore ou de jeton.** Si un secret apparaît
  dans un diff, le signaler avant commit.
- **Le dépôt est public** : aucun emploi du temps réel, aucune URL d'abonnement d'ENT, aucun nom
  d'étudiant ou d'enseignant réel. Les fixtures de `core/tests/fixtures/` sont anonymisées.

## Méthode de travail avec Jay

- Jay tranche les décisions produit ; Claude Code exécute, signale les cas limites, ne décide pas
  seul d'un choix produit.
- Le périmètre se coupe agressivement : proposer la version la plus simple d'abord, et dire ce qui
  a été simplifié.
- **En cas de doute sur une instruction, demander avant d'agir** plutôt que d'interpréter en silence.
- Livraison : une fois un travail vérifié, aller jusqu'au bout sans demander — monter la version
  (`Cargo.toml` du workspace, `versionCode` et `versionName` dans `android/app/build.gradle.kts`),
  commit, push sur `main`, tag `vX.Y.Z` poussé (la CI construit et publie l'APK signé).

## Règle impérative : TOUJOURS RÉPONDRE EN FRANÇAIS

Toutes les réponses à Jay sont en français, y compris après une reprise de session (le résumé de
contexte est en anglais : ce n'est pas une raison pour changer de langue).

## Règle impérative : PARLER SIMPLEMENT — Jay est débutant

> « Explique-moi les choses clairement et simplement, en gardant en tête que je suis débutant. »

C'est une condition pour que Jay puisse décider : une explication qu'on ne peut pas suivre retire
le pouvoir d'arbitrer.

1. **Le fait d'abord, en une phrase de tous les jours** — ce qui se passait, vu du téléphone. Le
   nom technique vient après, s'il sert encore.
2. **Une image concrète** quand le mécanisme est invisible.
3. **Distinguer explicitement les mots qui se ressemblent** (« app fermée » ≠ « app tuée par
   Android »).
4. **Dire ce que ça change pour Jay.**
5. **Un nom de fichier ou de fonction n'explique rien** : le citer pour retrouver l'endroit, après
   l'explication, jamais à sa place.

À éviter : empiler les termes techniques, répondre par un tableau de symboles à « qu'est-ce que
c'est ? », confondre court et simple, cacher une incertitude derrière du vocabulaire.

Le test avant d'envoyer : *« si Jay ne connaissait pas ce mot, sa question serait-elle
répondue ? »* Et « je ne comprends pas » veut dire : recommencer autrement, pas reformuler plus fort.

---

## Avant de considérer une tâche terminée

1. `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` au vert ;
   l'APK se construit (`./gradlew assembleDebug`) si le Kotlin a changé
2. Toute correction de bug a un test qui la reproduit (règle 6 de l'architecture), quand elle
   touche au cœur
3. Rapport de session à jour
4. Aucun secret ni donnée personnelle réelle dans le diff
5. `docs/parties-natives-par-os.md` reflète le code natif
