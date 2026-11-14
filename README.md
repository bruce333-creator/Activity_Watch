# Activity Tracker

Petit traceur d'activité pour Windows 10/11, écrit en Rust : **combien de temps vous passez sur votre PC**, et dans quelle application, avec un tableau de bord bien-être et un **Wrapped** chaque semaine.

- Un seul `.exe`, sans installation ni dépendance (runtime C liée statiquement), avec son icône.
- Pas de hook clavier/souris, pas de GPU, pas de réseau : le tableau de bord n'écoute que sur `127.0.0.1`.
- Échantillonnage toutes les 2 s par un thread endormi, priorité « inférieure à la normale » + EcoQoS (Windows 11).
- Les données restent chez vous : `%APPDATA%\ActivityTracker\activity.db` (SQLite).

## Télécharger le .exe

1. Sur GitHub, ouvrez l'onglet **Actions**, puis le workflow **build**.
2. Cliquez sur la dernière exécution réussie (coche verte).
3. En bas de la page, section **Artifacts**, téléchargez **activity-tracker** (un `.zip` contenant `activity-tracker.exe`).
4. Placez l'exe dans un dossier à vous, par ex. `C:\Outils\ActivityTracker\` (le fichier `config.toml` sera créé à côté au premier lancement).

Pour une version publiée : poussez un tag `v0.1.0` (par ex.) ; l'exe est alors joint à la **Release** GitHub correspondante.

> Windows SmartScreen peut avertir au premier lancement car l'exe n'est pas signé : « Informations complémentaires » → « Exécuter quand même ».

## Utilisation

Double-cliquez sur `activity-tracker.exe`. Le logo (horloge violet/cyan) apparaît dans la zone de notification (il peut être caché sous la flèche ^). Clic droit :

- **Open stats** : ouvre le tableau de bord (`http://127.0.0.1:8765/`).
- **Weekly Wrapped** : ouvre directement le bilan de la semaine.
- **Pause tracking** / **Resume tracking** : met en pause (l'icône devient grise).
- **Run at Windows startup** : coché = le traceur démarre avec Windows (cliquer pour activer/désactiver).
- **Quit** : enregistre la session en cours et quitte.

Relancer l'exe alors qu'il tourne déjà ouvre simplement le tableau de bord (une seule instance à la fois).

### Le tableau de bord

**Aujourd'hui / Semaine / Mois** :
- temps d'écran total, comparé à la même période précédente (hier à la même heure, la semaine dernière au même moment…) ;
- anneau d'objectif (`daily_goal_hours`) : temps du jour, ou moyenne par jour pour la semaine/le mois ;
- **bien-être** : nombre de pauses, plus longue période sans pause (alerte au-delà de 90 min), heure de début et de fin de journée, temps passé après 23 h ;
- « Ta journée » : la frise de vos applications heure par heure, puis les minutes par heure (ou les heures par jour, avec la ligne d'objectif) ;
- la liste des applications avec **leur vraie icône** (extraite de leur .exe).

**✨ Wrapped** (la semaine dernière par défaut, flèches pour naviguer) : votre « profil » de la semaine (Marathonien, Oiseau de nuit, Lève-tôt, Fidèle à Maya, Zen, Équilibré…), le temps total et l'évolution, l'app n°1, le top 5, le jour le plus chargé, l'heure de pointe, le record sans pause, les jours au-dessus de l'objectif.

La page se rafraîchit chaque minute ; la session en cours est écrite sur disque toutes les 60 s.

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
| `ignored_apps` | `["lockapp.exe"]` | Applications jamais enregistrées. |
| `web_port` | `8765` | Port du tableau de bord (toujours sur `127.0.0.1`). |
| `[friendly_names]` | liste intégrée | Nom affiché par processus. **Deux processus avec le même nom sont fusionnés.** Vos entrées s'ajoutent à la liste intégrée (Maya, Blender, Photoshop, Premiere Pro, After Effects, Houdini, ZBrush, Nuke, Substance Painter, Chrome…). |

Les anciennes clés `render_processes`, `render_cpu_threshold` et `three_d_apps` (version 0.1) sont simplement ignorées.

Les noms de processus ne sont pas sensibles à la casse. Pour trouver le nom exact d'un logiciel : Gestionnaire des tâches → onglet **Détails**. Premiere Pro s'appelle `Adobe Premiere Pro.exe` et After Effects `AfterFX.exe`.

Exemple :

```toml
idle_threshold_secs = 300
daily_goal_hours = 7

[friendly_names]
"houdinifx.exe" = "Houdini"
"mon-outil-maya.exe" = "Maya"   # fusionné avec Maya
```

## Démarrage automatique avec Windows

Le plus simple : clic droit sur l'icône → **Run at Windows startup**. Ou, depuis une invite de commandes dans le dossier de l'exe :

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

Fait tourner le collecteur 15 secondes (sur une base en mémoire, vos données ne sont pas touchées), puis affiche : application au premier plan détectée (nom et chemin), secondes d'inactivité, verrouillage, démarrage automatique, extraction d'icône, icônes de la zone de notification, mémoire (working set et pic) et temps CPU consommé. Code de sortie 0 si tout s'est bien passé.

### En cas de problème, envoyez-moi

1. La sortie complète de `activity-tracker.exe --selftest` (copier-coller depuis la console).
2. Le fichier `activity-tracker.log` après avoir reproduit le problème avec `--debug-log`.
3. Votre `config.toml`.
4. Une description : ce que vous faisiez, ce que vous attendiez, ce que le tableau de bord affiche. Si une application apparaît sous un mauvais nom, indiquez le nom exact de son processus (Gestionnaire des tâches → Détails).

## Compiler soi-même

```
cargo test
cargo build --release
```

Le résultat est `target\release\activity-tracker.exe`. Le code Win32 se trouve uniquement dans `src/win.rs` ; toute la logique (sessions, inactivité, SQLite, statistiques, bien-être, Wrapped, JSON) est indépendante de la plateforme et testée sous Linux comme sous Windows.

| Fichier | Rôle |
|---|---|
| `src/collector.rs` | Machine d'états actif / inactif, création des sessions, écriture toutes les 60 s |
| `src/store.rs` | SQLite (WAL), une ligne par session |
| `src/stats.rs` | Jour / semaine / mois, fusion des apps, bien-être, Wrapped |
| `src/png.rs` | Encodeur PNG minimal pour les icônes d'applications |
| `src/server.rs` | Tableau de bord local (tiny_http, accept bloquant) |
| `src/config.rs` | `config.toml` |
| `src/probe.rs` | Interface `SystemProbe` (+ sonde factice pour les tests) |
| `src/win.rs` | Win32 : fenêtre au premier plan, inactivité, verrouillage, icônes des .exe, zone de notification, registre |
| `assets/` | Page du tableau de bord, logo et icônes (intégrés dans l'exe via `build.rs`) |
