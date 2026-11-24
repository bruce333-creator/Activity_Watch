//! Notifications and tray texts (FR / EN). Platform independent.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    /// `setting` is the config value: English unless "fr" is asked for.
    pub fn resolve(setting: &str) -> Lang {
        if setting.trim().eq_ignore_ascii_case("fr") {
            Lang::Fr
        } else {
            Lang::En
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// No break for this many minutes.
    Break { minutes: i64 },
    /// Daily limit of an app reached.
    AppLimit { app: String, minutes: i64 },
    /// Daily screen-time goal reached.
    Goal { minutes: i64 },
    /// A new week started: last week's Wrapped is ready.
    Wrapped,
    /// A newer version is available (click installs it).
    Update { version: String },
    /// The app restarted on a new version.
    Updated { version: String },
    /// Download or replacement failed.
    UpdateFailed,
}

fn dur(lang: Lang, minutes: i64) -> String {
    let (h, m) = (minutes / 60, minutes % 60);
    match (lang, h, m) {
        (_, 0, m) => format!("{m} min"),
        (_, h, 0) => format!("{h} h"),
        (Lang::Fr, h, m) => format!("{h} h {m:02}"),
        (Lang::En, h, m) => format!("{h} h {m} min"),
    }
}

impl Notice {
    /// (title, body) of the Windows notification.
    pub fn text(&self, lang: Lang) -> (String, String) {
        use Lang::*;
        match (self, lang) {
            (Notice::Break { minutes }, Fr) => (
                "C'est l'heure d'une pause ☕".into(),
                format!(
                    "{} devant l'écran sans pause. Lève-toi, étire-toi, regarde au loin quelques minutes.",
                    dur(lang, *minutes)
                ),
            ),
            (Notice::Break { minutes }, En) => (
                "Time for a break ☕".into(),
                format!(
                    "{} at the screen without a break. Stand up, stretch, look into the distance for a few minutes.",
                    dur(lang, *minutes)
                ),
            ),
            (Notice::AppLimit { app, minutes }, Fr) => (
                format!("Limite atteinte : {app}"),
                format!("Tu as passé {} sur {app} aujourd'hui.", dur(lang, *minutes)),
            ),
            (Notice::AppLimit { app, minutes }, En) => (
                format!("Limit reached: {app}"),
                format!("You spent {} in {app} today.", dur(lang, *minutes)),
            ),
            (Notice::Goal { minutes }, Fr) => (
                "Objectif du jour atteint".into(),
                format!(
                    "{} d'écran aujourd'hui. Et si tu t'arrêtais là ?",
                    dur(lang, *minutes)
                ),
            ),
            (Notice::Goal { minutes }, En) => (
                "Daily goal reached".into(),
                format!(
                    "{} of screen time today. Maybe call it a day?",
                    dur(lang, *minutes)
                ),
            ),
            (Notice::Wrapped, Fr) => (
                "Ton Wrapped est prêt ✨".into(),
                "Découvre le bilan de ta semaine.".into(),
            ),
            (Notice::Wrapped, En) => (
                "Your Wrapped is ready ✨".into(),
                "See how your week went.".into(),
            ),
            (Notice::Update { version }, Fr) => (
                format!("Mise à jour {version} disponible"),
                "Cliquez ici pour l'installer. Ounce redémarrera tout seul, sans perdre vos données.".into(),
            ),
            (Notice::Update { version }, En) => (
                format!("Update {version} available"),
                "Click here to install it. Ounce restarts by itself, your data is kept.".into(),
            ),
            (Notice::Updated { version }, Fr) => (
                format!("Ounce {version} installé"),
                "La mise à jour est terminée.".into(),
            ),
            (Notice::Updated { version }, En) => (
                format!("Ounce {version} installed"),
                "The update is complete.".into(),
            ),
            (Notice::UpdateFailed, Fr) => (
                "La mise à jour a échoué".into(),
                "Réessayez plus tard, ou téléchargez la nouvelle version sur la page du projet.".into(),
            ),
            (Notice::UpdateFailed, En) => (
                "Update failed".into(),
                "Try again later, or download the new version from the project page.".into(),
            ),
        }
    }

    /// Dashboard page opened when the notification is clicked.
    pub fn page(&self) -> &'static str {
        match self {
            Notice::Wrapped => "#wrapped",
            Notice::Update { .. } => "#update",
            _ => "#today",
        }
    }
}

/// Tray menu labels.
pub struct Menu {
    pub open: &'static str,
    pub wrapped: &'static str,
    pub pause: &'static str,
    pub resume: &'static str,
    pub startup: &'static str,
    pub quit: &'static str,
    pub settings: &'static str,
    pub widget: &'static str,
    pub widget_hide: &'static str,
    pub paused_short: &'static str,
    /// "Install update {version}"
    pub update: &'static str,
    pub paused_tip: &'static str,
}

pub fn menu(lang: Lang) -> Menu {
    match lang {
        Lang::Fr => Menu {
            open: "Ouvrir Ounce",
            wrapped: "Wrapped de la semaine",
            pause: "Mettre en pause",
            resume: "Reprendre le suivi",
            startup: "Lancer au démarrage de Windows",
            quit: "Quitter",
            settings: "Modifier les réglages…",
            widget: "Afficher dans la barre des tâches",
            widget_hide: "Masquer de la barre des tâches",
            paused_short: "En pause",
            update: "Installer la mise à jour {}",
            paused_tip: "Ounce (en pause)",
        },
        Lang::En => Menu {
            open: "Open Ounce",
            wrapped: "Weekly Wrapped",
            pause: "Pause tracking",
            resume: "Resume tracking",
            startup: "Run at Windows startup",
            quit: "Quit",
            settings: "Edit settings…",
            widget: "Show on the taskbar",
            widget_hide: "Hide from the taskbar",
            paused_short: "Paused",
            update: "Install update {}",
            paused_tip: "Ounce (paused)",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_resolution() {
        assert_eq!(Lang::resolve("en"), Lang::En);
        assert_eq!(Lang::resolve("auto"), Lang::En);
        assert_eq!(Lang::resolve(""), Lang::En);
        assert_eq!(Lang::resolve(" FR "), Lang::Fr);
    }

    #[test]
    fn texts_fit_windows_limits() {
        // Windows truncates notification titles at 63 and bodies at 255 UTF-16 units.
        for lang in [Lang::Fr, Lang::En] {
            for n in [
                Notice::Break { minutes: 95 },
                Notice::AppLimit {
                    app: "Adobe Substance 3D Painter".into(),
                    minutes: 120,
                },
                Notice::Goal { minutes: 480 },
                Notice::Wrapped,
                Notice::Update {
                    version: "10.10.10".into(),
                },
                Notice::Updated {
                    version: "10.10.10".into(),
                },
                Notice::UpdateFailed,
            ] {
                let (t, b) = n.text(lang);
                assert!(
                    t.encode_utf16().count() < 64 && b.encode_utf16().count() < 256,
                    "{t} / {b}"
                );
            }
        }
        assert_eq!(
            Notice::Break { minutes: 95 }
                .text(Lang::Fr)
                .1
                .split(' ')
                .take(3)
                .collect::<Vec<_>>(),
            ["1", "h", "35"]
        );
    }
}
