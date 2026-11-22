# Activity Tracker

*by Enzo Freminet · ef tools*

Traceur de temps d'écran pour Windows 10/11, écrit en Rust. Il mesure **combien de temps vous passez sur votre PC** et dans chaque application, avec un tableau de bord bien-être, un **Wrapped** chaque semaine et le temps par projet pour les artistes 3D (Maya, Blender, Houdini…).

- Un seul `.exe` d'environ 2 Mo, sans installation ni dépendance (runtime C liée statiquement).
- Pas de hook clavier/souris, pas de GPU, pas de réseau : le tableau de bord n'écoute que sur `127.0.0.1`, aucune télémétrie.
- Échantillonnage toutes les 2 s par un thread endormi, priorité « inférieure à la normale » + EcoQoS (Windows 11).
- Les données restent chez vous : `%APPDATA%\ActivityTracker\activity.db` (SQLite).

## Télécharger le .exe

1. Sur GitHub, ouvrez l'onglet **Actions**, puis le workflow **build**.
2. Cliquez sur la dernière exécution réussie (coche verte).
3. En bas de la page, section **Artifacts**, téléchargez **activity-tracker** (un `.zip` contenant `activity-tracker.exe`).
4. Placez l'exe dans un dossier à vous, par ex. `C:\Outils\ActivityTracker\` (le fichier `config.toml` sera créé à côté au premier lancement).

Pour une version publiée : poussez un tag `v0.4.0` (par ex.) ; l'exe est alors joint à la **Release** GitHub correspondante (voir « Distribuer »).

> Windows SmartScreen peut avertir au premier lancement car l'exe n'est pas signé : « Informations complémentaires » → « Exécuter quand même ».

## Mettre à jour

**Automatiquement** (à partir de la 0.5) : une fois par jour, Activity Tracker regarde s'il existe une nouvelle version sur GitHub. Si oui :

- une notification « Mise à jour X disponible » apparaît : **cliquez dessus** pour l'installer ;
- ou clic droit sur l'icône → **Installer la mise à jour X** ;
- ou le bouton **Installer** dans le bandeau en haut de la fenêtre.

L'appli télécharge le nouvel exe, le met à la place de l'ancien et redémarre toute seule (quelques secondes). Rien n'est installé sans votre clic. Vos données, réglages, noms et logos sont conservés. Pour désactiver la vérification : `check_updates = false`.

**À la main** (versions 0.4 et avant, ou si l'exe est dans un dossier protégé comme `C:\Program Files`) :

1. Clic droit sur l'icône → **Quitter**. Cela enregistre la session en cours.
2. Remplacez l'ancien `activity-tracker.exe` par le nouveau, **dans le même dossier** et sous le même nom.
3. Double-cliquez sur le nouvel exe.

Vos données (historique, noms et logos personnalisés) sont dans `%APPDATA%\ActivityTracker\` et ne sont jamais touchées par une mise à jour. Le démarrage automatique continue de marcher puisque le chemin de l'exe ne change pas. La version installée est affichée en bas de la fenêtre et dans Propriétés → Détails de l'exe.

## Utilisation

Double-cliquez sur `activity-tracker.exe`. Le logo (horloge blanche sur fond noir) apparaît dans la zone de notification ; il peut être caché sous la flèche ^. Faites-le glisser dans la barre pour le garder visible.

- **Clic gauche** : ouvre Activity Tracker dans sa propre fenêtre.
- **Clic droit** : *Ouvrir Activity Tracker*, *Wrapped de la semaine*, *Mettre en pause / Reprendre le suivi* (l'icône devient grise), *Lancer au démarrage de Windows* (case à cocher), *Quitter*.

Les menus, notifications et le tableau de bord sont en **français ou en anglais** selon la langue de Windows (réglable avec `language`).

Relancer l'exe alors qu'il tourne déjà ouvre simplement la fenêtre (une seule instance à la fois).

### La fenêtre

L'interface s'ouvre dans une **fenêtre d'application** (mode « app » de Microsoft Edge, présent sur tout Windows 10/11 : pas d'onglets, pas de barre d'adresse). Cela ne coûte rien quand la fenêtre est fermée et ne grossit pas l'exe. Si Edge est absent, la page s'ouvre dans le navigateur par défaut, à l'adresse `http://127.0.0.1:8765/`.

- **Aujourd'hui / Semaine / Mois** :
  - temps d'écran comparé à la même période précédente (hier à la même heure, etc.) ;
  - anneau d'objectif ;
  - frise de la journée et barres par heure ou par jour ;
  - applications avec **leur vraie icône** et la variation par appli (« +1 h 20 », « −35 min ») ;
  - **projets** (voir plus bas) ;
  - **bien-être** : pauses, plus long sans pause, début et fin de journée, temps après 23 h ;
  - **focus** : plus longue concentration sur une seule appli, changements d'appli par heure ;
  - **limites par appli**.
- **Année** : un calendrier (une case par jour, plus clair = plus de temps), la **série créative** 🔥 (jours d'affilée avec au moins 30 min sur vos apps créatives), la **série sous l'objectif** 🌿, le jour record et la moyenne.
- **✨ Wrapped** : votre profil de la semaine (Marathonien, Oiseau de nuit, Lève-tôt, Fidèle à Maya, Concentration profonde, Zen, Équilibré…), le total et l'évolution, l'app n°1, le top 5, vos projets, ce qui a monté ou baissé, le jour le plus chargé et l'heure de pointe. Le bouton **Partager l'image** télécharge une image 1080×1350 prête pour les réseaux sociaux.
- **Exporter en CSV** (en bas de page) : toutes les sessions (début, fin, minutes, appli, processus, projet), à ouvrir dans Excel.

La page se rafraîchit chaque minute ; la session en cours est écrite sur disque toutes les 60 s, et immédiatement quand Windows s'éteint.

### Renommer une appli, changer son logo

Bouton **✎** au survol d'une appli, ou onglet **Applis** (toutes les applis déjà vues) → **Personnaliser** :
- **nom affiché** : il s'applique partout (listes, Wrapped, limites, export) et à tous les processus de l'appli ;
- **logo** : PNG, JPG, SVG, ICO ou WebP, automatiquement mis à 128×128 px.

Pour revenir en arrière, utilisez « Remettre le nom d'origine » et « Remettre le logo d'origine ». Donner le même nom à deux applis les regroupe (par exemple Chrome et Edge en « Web »).

Tout est enregistré dans la base (`%APPDATA%\ActivityTracker\activity.db`) : c'est permanent, y compris après une mise à jour ou un redémarrage.

### Notifications

Notifications Windows natives, désactivables avec `notifications = false` :

- **Pause** ☕ après `break_reminder_minutes` (60) sans vraie pause, puis à nouveau toutes les 60 min tant que vous n'en faites pas. Une pause de 5 min ou plus remet le compteur à zéro.
- **Limite par appli** atteinte (`[app_limits]`), une fois par jour.
- **Objectif du jour** atteint, une fois par jour.
- **Wrapped prêt** ✨ au premier usage d'une nouvelle semaine ; un clic l'ouvre. Avec `wrapped_auto_open = true`, il s'ouvre directement.

### Temps par projet (artistes 3D)

Le titre de la fenêtre active est lu **uniquement** pour y trouver le nom d'un fichier de projet connu : `.mb .ma .blend .hip .hipnc .nk .aep .prproj .psd .spp .ztl .uproject .c4d .max .usd`… Seul le nom du fichier est enregistré (par exemple `robot_v03.mb`), jamais le reste du titre. Les fenêtres annexes (Script Editor, Hypershade, boîtes de dialogue) restent comptées sur la scène ouverte.

### Comment le temps est compté

- **Actif** : vous avez utilisé le clavier ou la souris depuis moins de `idle_threshold_secs` (180 s par défaut). Le temps est attribué au **processus** de la fenêtre au premier plan, jamais au titre de la fenêtre : toutes les fenêtres de Maya (vue 3D, Script Editor, Hypershade, Render View, boîtes de dialogue…) comptent comme Maya.
- **Une application = un nom** : les processus qui portent le même nom affiché sont fusionnés. `maya.exe`, `mayapy.exe`, `mayabatch.exe` et `render.exe` sont tous comptés comme **Maya**. Quand Maya ne répond plus (« Ne répond pas »), Windows affiche une fenêtre fantôme appartenant à `dwm.exe` : ce temps reste attribué à Maya.
- **Inactif** : au-delà du seuil, ou session verrouillée (Win+L). La session se termine au moment de la **dernière** saisie : les 3 minutes d'attente ne sont pas comptées. Écran éteint = pas de saisie = pas compté. Une mise en veille du PC coupe la session au dernier échantillon.
- **Pause** (bien-être) : un trou d'au moins `break_minutes` (5 min) entre deux périodes d'activité de la même journée. Les journées vont de 5 h à 5 h : finir à 1 h du matin compte dans la soirée de la veille.

## Configuration (`config.toml`)

Créé automatiquement à côté de l'exe au premier lancement. Modifiez-le puis relancez le traceur (Quit puis double-clic). En cas d'erreur de syntaxe, les valeurs par défaut sont utilisées (et l'erreur est notée dans le log `--debug-log`).

| Clé | Défaut | Rôle |
|---|---|---|
| `poll_interval_secs` | `2` | Intervalle entre deux échantillons (1 à 60 s). |
| `idle_threshold_secs` | `180` | Secondes sans clavier/souris avant de considérer que vous êtes absent. |
| `daily_goal_hours` | `8.0` | Objectif (ou limite) de temps d'écran par jour. `0` = pas d'objectif. |
| `break_minutes` | `5` | Durée minimale d'un trou pour compter comme une pause. |
| `notifications` | `true` | Active les notifications Windows. |
| `break_reminder_minutes` | `60` | Rappel de pause après X min sans pause. `0` = jamais. |
| `wrapped_auto_open` | `false` | `true` = le Wrapped s'ouvre tout seul en début de semaine (sinon : notification). |
| `language` | `"auto"` | `"auto"` (langue de Windows), `"fr"` ou `"en"`. |
| `focus_apps` | Maya, Blender, Houdini, ZBrush, Substance Painter, Photoshop, Nuke, After Effects, Premiere Pro, Unreal Engine, RenderMan | Vos apps créatives (noms affichés), pour la série 🔥. |
| `streak_minutes` | `30` | Minutes minimum sur ces apps pour qu'un jour compte dans la série. |
| `ignored_apps` | `["lockapp.exe"]` | Applications jamais enregistrées. |
| `web_port` | `8765` | Port du tableau de bord (toujours sur `127.0.0.1`). |
| `check_updates` | `true` | Vérifie une fois par jour s'il existe une nouvelle version (une requête à `api.github.com`, rien d'autre n'est envoyé). |
| `[app_limits]` | vide | Limite quotidienne en minutes par appli (nom affiché), ex. `"Discord" = 60`. |
| `[friendly_names]` | liste intégrée | Nom affiché par processus. **Deux processus avec le même nom sont fusionnés.** Vos entrées s'ajoutent à la liste intégrée (Maya, Blender, Photoshop, Premiere Pro, After Effects, Houdini, ZBrush, Nuke, Substance Painter, Chrome…). |

Un `config.toml` créé par une ancienne version reste valable : les nouvelles clés prennent leur valeur par défaut. Pour voir toutes les options commentées, renommez votre fichier puis relancez le traceur, qui en recrée un complet.

Les anciennes clés `render_processes`, `render_cpu_threshold` et `three_d_apps` (version 0.1) sont simplement ignorées.

Les noms de processus ne sont pas sensibles à la casse. Pour trouver le nom exact d'un logiciel : Gestionnaire des tâches → onglet **Détails**. Premiere Pro s'appelle `Adobe Premiere Pro.exe` et After Effects `AfterFX.exe`.

Exemple :

```toml
idle_threshold_secs = 300
daily_goal_hours = 7
break_reminder_minutes = 75

[app_limits]
"Discord" = 60
"Chrome" = 120

[friendly_names]
"houdinifx.exe" = "Houdini"
"mon-outil-maya.exe" = "Maya"   # fusionné avec Maya
```

## Démarrage automatique avec Windows

Le plus simple : clic droit sur l'icône → **Lancer au démarrage de Windows**. Ou, depuis une invite de commandes dans le dossier de l'exe :

```
activity-tracker.exe --install-autostart
```

Cela ajoute une entrée dans `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (votre compte uniquement, pas besoin d'être administrateur) qui pointe vers l'emplacement **actuel** de l'exe. Si vous déplacez l'exe, relancez la commande. Pour retirer :

```
activity-tracker.exe --uninstall-autostart
```

## Diagnostic

### `--debug-log`

```
activity-tracker.exe --debug-log
```

Écrit `activity-tracker.log` à côté de l'exe : démarrage et configuration, chaque changement d'état (début/fin de session avec le nom de l'application, pause, démarrage automatique), les erreurs, et la mémoire utilisée (working set) toutes les minutes. Pour l'utiliser au démarrage automatique, lancez une fois `--install-autostart`, puis modifiez la valeur `ActivityTracker` dans `regedit` pour ajouter ` --debug-log` après le chemin.

### `--selftest`

```
activity-tracker.exe --selftest
```

Fait tourner le collecteur 15 secondes sur une base en mémoire (vos données ne sont pas touchées), puis affiche :\n\n- l'application au premier plan (nom et chemin) et la scène ou le projet détecté ;\n- les secondes d'inactivité et l'état de verrouillage ;\n- la langue, le démarrage automatique et la présence d'Edge pour la fenêtre ;\n- l'extraction d'icône et les icônes de la zone de notification ;\n- la mémoire (working set et pic) et le temps CPU consommé. Code de sortie 0 si tout s'est bien passé.

### En cas de problème, envoyez-moi

1. La sortie complète de `activity-tracker.exe --selftest` (copier-coller depuis la console).
2. Le fichier `activity-tracker.log` après avoir reproduit le problème avec `--debug-log`.
3. Votre `config.toml`.
4. Une description : ce que vous faisiez, ce que vous attendiez, ce que le tableau de bord affiche. Si une application apparaît sous un mauvais nom, indiquez le nom exact de son processus (Gestionnaire des tâches → Détails).

## Distribuer

### Publier une nouvelle version

1. Changez `version` dans `Cargo.toml` (par ex. `0.5.0` → `0.6.0`), commitez et poussez.
2. Créez le tag correspondant et poussez-le :

   ```
   git tag v0.6.0
   git push origin v0.6.0
   ```

GitHub Actions compile l'exe, le teste et crée la **Release** avec l'exe et des notes de version. Dès qu'elle est publiée, toutes les applis installées la proposent dans les 24 h. C'est gratuit (Actions et Releases sont gratuits pour un dépôt public).

Règles :
- le tag doit être exactement `v` + la version de `Cargo.toml`, sinon le build refuse de publier (une appli qui se croirait toujours en retard se mettrait à jour en boucle) ;
- le dépôt doit rester **public** : les applis interrogent GitHub sans compte ;
- l'exe joint doit garder le nom `activity-tracker.exe` ;
- une Release marquée « pre-release » ou en brouillon n'est pas proposée : pratique pour tester avant de diffuser.

**Lien de téléchargement permanent** (toujours la dernière version, idéal pour le site) : `https://github.com/bruce333-creator/Activity_Watch/releases/latest/download/activity-tracker.exe`.

### SmartScreen et antivirus

Voir [Signer l'exe gratuitement](#signer-lexe-gratuitement) plus bas.

- **Antivirus** : en cas de faux positif, soumettez l'exe à Microsoft (Microsoft Security Intelligence, « Submit a file », gratuit).
- **Plus tard** : publication sur `winget` (le gestionnaire de paquets de Windows), qui permet `winget install`.

### Signer l'exe gratuitement

- **SignPath Foundation** : signature gratuite pour les projets open source. Il faut une licence open source (MIT par exemple) dans le dépôt, un dépôt public et un build par GitHub Actions. Après acceptation, la signature s'ajoute comme une étape du workflow. Le certificat est au nom de « SignPath Foundation ». Il a déjà une réputation chez Microsoft, donc l'avertissement disparaît vite. Ce n'est pas garanti dès le premier jour.
- **Microsoft Store** : le compte développeur individuel est gratuit et le Store signe le paquet MSIX. Il n'y a alors aucun avertissement SmartScreen. Cette version demanderait des adaptations : c'est le Store qui gère les mises à jour (`check_updates = false`), et le lancement au démarrage passe par une déclaration du paquet plutôt que par le registre.
- **Sans signature** : les mises à jour automatiques ne déclenchent pas SmartScreen. Seul le premier téléchargement depuis un navigateur est concerné, et l'avertissement diminue à mesure que l'exe est téléchargé.
- Un **certificat auto-signé** ne sert à rien : Windows ne lui fait pas confiance.

## Licences tierces

La police [Manrope](https://github.com/sharanda/manrope) est intégrée dans l'exe sous licence SIL Open Font License 1.1 (`assets/fonts/OFL-Manrope.txt`), qui autorise sa redistribution avec le logiciel.

## Compiler soi-même

```
cargo test
cargo build --release
```

Le résultat est `target\release\activity-tracker.exe`. Le code Win32 se trouve uniquement dans `src/win.rs` ; toute la logique (sessions, inactivité, SQLite, statistiques, bien-être, Wrapped, JSON) est indépendante de la plateforme et testée sous Linux comme sous Windows.

| Fichier | Rôle |
|---|---|
| `src/collector.rs` | Machine d'états actif / inactif, sessions, projets, rappels de pause, limites, objectif, Wrapped |
| `src/store.rs` | SQLite (WAL), une ligne par session |
| `src/stats.rs` | Jour / semaine / mois / année, fusion des apps, projets, bien-être, focus, séries, Wrapped |\n| `src/project.rs` | Nom de la scène / du projet à partir du titre de fenêtre |\n| `src/notice.rs` | Textes des notifications et du menu (FR / EN) |
| `src/png.rs` | Encodeur PNG minimal pour les icônes d'applications |
| `src/server.rs` | Tableau de bord local (tiny_http, accept bloquant), export CSV, noms et logos personnalisés |
| `src/config.rs` | `config.toml` |
| `src/probe.rs` | Interface `SystemProbe` (+ sonde factice pour les tests) |
| `src/win.rs` | Win32 : fenêtre au premier plan et titre, inactivité, verrouillage, icônes des .exe, zone de notification et notifications, fenêtre Edge, registre |
| `assets/` | Page du tableau de bord, logo, icônes et police Manrope (intégrés dans l'exe ; `build.rs` ajoute aussi les infos de version) |
