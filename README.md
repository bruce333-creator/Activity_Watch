# Activity Tracker

Petit traceur d'activité pour Windows 10/11, écrit en Rust : combien de temps vous passez sur le PC et dans chaque application (Maya, Blender, Photoshop, Premiere Pro, After Effects, RenderMan…), avec le temps de **rendu** compté à part.

- Un seul `.exe`, sans installation ni dépendance (runtime C liée statiquement).
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

Double-cliquez sur `activity-tracker.exe`. Une icône ronde bleue apparaît dans la zone de notification (elle peut être cachée sous la flèche ^). Clic droit :

- **Open stats** : ouvre le tableau de bord dans le navigateur (`http://127.0.0.1:8765/`).
- **Pause tracking** / **Resume tracking** : met en pause (l'icône devient grise).
- **Quit** : enregistre la session en cours et quitte.

Relancer l'exe alors qu'il tourne déjà ouvre simplement le tableau de bord (une seule instance à la fois).

Le tableau de bord propose **Aujourd'hui / Semaine / Mois** : temps actif total, temps de rendu, part des logiciels 3D, temps par application (graphique + tableau) et chronologie (par heure pour aujourd'hui, par jour sinon). Il se rafraîchit chaque minute ; la session en cours est écrite sur disque toutes les 60 s.

### Comment le temps est compté

- **Actif** : vous avez utilisé le clavier ou la souris depuis moins de `idle_threshold_secs` (180 s par défaut). Le temps est attribué au **processus** de la fenêtre au premier plan (`maya.exe`), pas au titre de la fenêtre.
- **Inactif** : au-delà du seuil, ou session verrouillée (Win+L). La session active se termine au moment de la **dernière** saisie : les 3 minutes d'attente ne sont pas comptées. L'écran éteint n'est jamais compté, puisqu'il n'y a pas de saisie. Une mise en veille du PC coupe la session au dernier échantillon.
- **Rendu** : quand vous êtes inactif et qu'un processus de `render_processes` utilise plus de `render_cpu_threshold` % du CPU total (même échelle que le Gestionnaire des tâches), ce temps est compté en « rendu ». Pendant que vous travaillez, le temps reste « actif » dans l'application au premier plan.

## Configuration (`config.toml`)

Créé automatiquement à côté de l'exe au premier lancement. Modifiez-le puis relancez le traceur (Quit puis double-clic). En cas d'erreur de syntaxe, les valeurs par défaut sont utilisées (et l'erreur est notée dans le log `--debug-log`).

| Clé | Défaut | Rôle |
|---|---|---|
| `poll_interval_secs` | `2` | Intervalle entre deux échantillons (1 à 60 s). |
| `idle_threshold_secs` | `180` | Secondes sans clavier/souris avant de considérer que vous êtes absent. |
| `render_cpu_threshold` | `50.0` | % CPU (de toute la machine) au-dessus duquel un processus de rendu compte. |
| `render_processes` | `["prman.exe", "mayabatch.exe", "render.exe"]` | Processus de rendu surveillés (seuls ceux-ci sont mesurés). |
| `three_d_apps` | Maya, Blender, Photoshop, After Effects, Premiere Pro, rendus | Applications comptées comme « 3D / création » dans le graphique de répartition. |
| `ignored_apps` | `["lockapp.exe"]` | Applications jamais enregistrées. |
| `web_port` | `8765` | Port du tableau de bord (toujours sur `127.0.0.1`). |
| `[friendly_names]` | `"maya.exe" = "Maya"`, … | Nom affiché pour chaque processus. |

Les noms de processus ne sont pas sensibles à la casse. Pour trouver le nom exact d'un logiciel : Gestionnaire des tâches → onglet **Détails**. Premiere Pro s'appelle `Adobe Premiere Pro.exe` et After Effects `AfterFX.exe`.

Exemple :

```toml
idle_threshold_secs = 300
render_processes = ["prman.exe", "mayabatch.exe", "render.exe", "blender.exe"]

[friendly_names]
"maya.exe" = "Maya"
"houdinifx.exe" = "Houdini"
```

## Démarrage automatique avec Windows

Depuis une invite de commandes (`cmd`) ou PowerShell, dans le dossier de l'exe :

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

Écrit `activity-tracker.log` à côté de l'exe : démarrage et configuration, chaque changement d'état (début/fin de session, application, actif/rendu, pause), les erreurs, et la mémoire utilisée (working set) toutes les minutes. Pour l'utiliser au démarrage automatique, lancez une fois `--install-autostart`, puis modifiez la valeur `ActivityTracker` dans `regedit` pour ajouter ` --debug-log` après le chemin.

### `--selftest`

```
activity-tracker.exe --selftest
```

Fait tourner le collecteur 15 secondes (sur une base en mémoire, vos données ne sont pas touchées), puis affiche : application au premier plan détectée, secondes d'inactivité, verrouillage, processus de rendu trouvés, mémoire (working set et pic) et temps CPU consommé. Code de sortie 0 si tout s'est bien passé.

### En cas de problème, envoyez-moi

1. La sortie complète de `activity-tracker.exe --selftest` (copier-coller depuis la console).
2. Le fichier `activity-tracker.log` après avoir reproduit le problème avec `--debug-log`.
3. Votre `config.toml`.
4. Une description : ce que vous faisiez, ce que vous attendiez, ce que le tableau de bord affiche. Pour un problème de rendu, indiquez le nom exact du processus (Gestionnaire des tâches → Détails) et son % CPU pendant le rendu.

## Compiler soi-même

```
cargo test
cargo build --release
```

Le résultat est `target\release\activity-tracker.exe`. Le code Win32 se trouve uniquement dans `src/win.rs` ; toute la logique (sessions, inactivité, rendu, SQLite, agrégation, JSON) est indépendante de la plateforme et testée sous Linux comme sous Windows.

| Fichier | Rôle |
|---|---|
| `src/collector.rs` | Machine d'états actif / inactif / rendu, création des sessions, écriture toutes les 60 s |
| `src/store.rs` | SQLite (WAL), une ligne par session |
| `src/stats.rs` | Agrégation jour / semaine / mois |
| `src/server.rs` | Tableau de bord local (tiny_http, accept bloquant) |
| `src/config.rs` | `config.toml` |
| `src/probe.rs` | Interface `SystemProbe` (+ sonde factice pour les tests) |
| `src/win.rs` | Win32 : fenêtre au premier plan, inactivité, verrouillage, CPU, icône de notification, registre |
| `assets/` | Page HTML et Chart.js 4.4.7 (licence MIT), intégrés dans l'exe |
