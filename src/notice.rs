//! Notifications and tray texts (FR / EN). Platform independent.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    /// `setting` is the config value ("auto" | "fr" | "en"); `system_is_french` comes from Windows.
    pub fn resolve(setting: &str, system_is_french: bool) -> Lang {
        match setting.to_ascii_lowercase().as_str() {
            "fr" => Lang::Fr,
            "en" => Lang::En,
            _ if system_is_french => Lang::Fr,
            _ => Lang::En,
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
        }
    }

    /// Dashboard page opened when the notification is clicked.
    pub fn page(&self) -> &'static str {
        match self {
            Notice::Wrapped => "#wrapped",
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
    pub paused_tip: &'static str,
}

pub fn menu(lang: Lang) -> Menu {
    match lang {
        Lang::Fr => Menu {
            open: "Ouvrir Activity Tracker",
            wrapped: "Wrapped de la semaine",
            pause: "Mettre en pause",
            resume: "Reprendre le suivi",
            startup: "Lancer au démarrage de Windows",
            quit: "Quitter",
            paused_tip: "Activity Tracker (en pause)",
        },
        Lang::En => Menu {
            open: "Open Activity Tracker",
            wrapped: "Weekly Wrapped",
            pause: "Pause tracking",
            resume: "Resume tracking",
            startup: "Run at Windows startup",
            quit: "Quit",
            paused_tip: "Activity Tracker (paused)",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_resolution() {
        assert_eq!(Lang::resolve("auto", true), Lang::Fr);
        assert_eq!(Lang::resolve("auto", false), Lang::En);
        assert_eq!(Lang::resolve("EN", true), Lang::En);
        assert_eq!(Lang::resolve("fr", false), Lang::Fr);
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
