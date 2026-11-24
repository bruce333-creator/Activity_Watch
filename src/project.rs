//! Extracts the open scene / project file from a window title. Platform independent.
//! Only the file name of a known project format is kept; other titles are never stored.

const EXTENSIONS: &[&str] = &[
    ".mb",
    ".ma",
    ".blend",
    ".hip",
    ".hipnc",
    ".hiplc",
    ".nk",
    ".aep",
    ".prproj",
    ".psd",
    ".psb",
    ".spp",
    ".ztl",
    ".zpr",
    ".uproject",
    ".c4d",
    ".max",
    ".kra",
    ".clip",
    ".usd",
    ".usda",
    ".fbx",
    ".obj",
];

/// "Autodesk Maya 2025: C:\proj\robot_v03.mb*" -> Some("robot_v03.mb").
pub fn project_from_title(title: &str) -> Option<String> {
    let lower = title.to_ascii_lowercase(); // ASCII-only: keeps byte positions identical
    // Pick the extension that appears last in the title (the file name is usually at the end).
    let (pos, ext) = EXTENSIONS
        .iter()
        .filter_map(|e| {
            let i = lower.rfind(e)?;
            // Must end a word: "scene.mb" but not "scene.mbx".
            let after = lower[i + e.len()..].chars().next();
            (after.is_none_or(|c| !c.is_alphanumeric())).then_some((i, *e))
        })
        .max_by_key(|(i, e)| (*i, e.len()))?;
    let end = pos + ext.len();
    // The file name starts after the last path separator / bracket / colon before it.
    let head = &title[..pos];
    let start = [
        head.rfind(['\\', '/', '[', '(', ':', '|', '"'])
            .map(|i| i + 1),
        head.rfind(" - ").map(|i| i + 3),
        head.rfind(" \u{2014} ").map(|i| i + " \u{2014} ".len()),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(0);
    let name = title[start..end].trim().trim_start_matches('*').trim();
    (name.len() > ext.len()).then(|| name.to_string())
}

/// Versions of the same scene share one project: "robot_v03.mb", "robot-v4.mb",
/// "robot v2 final.mb" and "robot (1).mb" are all "robot.mb".
pub fn project_group(name: &str) -> String {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    };
    let sep = |c: char| matches!(c, '_' | '-' | '.' | ' ');
    let mut s = stem.trim_end();
    loop {
        let lower = s.to_ascii_lowercase();
        let before = s.len();
        // Trailing words that mark a version rather than a different scene.
        for word in ["final", "wip", "copy", "old", "backup"] {
            if lower.ends_with(word) && lower.len() > word.len() {
                let rest = &s[..s.len() - word.len()];
                if rest.ends_with(sep) {
                    s = rest.trim_end_matches(sep);
                }
            }
        }
        // "(2)" copies made by Windows.
        if let Some(rest) = s.strip_suffix(')') {
            if let Some(i) = rest.rfind('(') {
                if i > 0 && rest[i + 1..].chars().all(|c| c.is_ascii_digit()) && rest.len() > i + 1
                {
                    s = rest[..i].trim_end();
                }
            }
        }
        // v03, v3, ver3, version3 after a separator.
        let digits = s.len() - s.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0 {
            let head = &s[..s.len() - digits];
            let lower_head = head.to_ascii_lowercase();
            for tag in ["version", "ver", "v"] {
                if lower_head.ends_with(tag) && head.len() > tag.len() {
                    let rest = &head[..head.len() - tag.len()];
                    if rest.ends_with(sep) && rest.len() > 1 {
                        s = rest.trim_end_matches(sep);
                        break;
                    }
                }
            }
        }
        if s.len() == before || s.is_empty() {
            break;
        }
    }
    if s.is_empty() {
        return name.to_string();
    }
    format!("{s}{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles() {
        let cases = [
            (
                "Autodesk Maya 2025: C:\\Projects\\Robot\\scenes\\robot_v03.mb*",
                Some("robot_v03.mb"),
            ),
            ("Autodesk MAYA 2024.2: D:/work/forest.ma", Some("forest.ma")),
            ("Autodesk Maya 2025: untitled*", None),
            ("Script Editor", None),
            ("Blender* [C:\\art\\donut.blend]", Some("donut.blend")),
            ("castle.blend - Blender 4.2", Some("castle.blend")),
            (
                "Adobe After Effects 2025 - C:\\video\\intro.aep *",
                Some("intro.aep"),
            ),
            (
                "Adobe Premiere Pro 2025 - C:\\edit\\clip final.prproj",
                Some("clip final.prproj"),
            ),
            (
                "Houdini FX 20.5 - /home/fx/smoke_sim.hipnc",
                Some("smoke_sim.hipnc"),
            ),
            ("YouTube - Google Chrome", None),
            ("my.mbx file", None),
            (
                "Adobe Photoshop 2025 - poster-v2.psd @ 50%",
                Some("poster-v2.psd"),
            ),
        ];
        for (title, want) in cases {
            assert_eq!(project_from_title(title).as_deref(), want, "{title}");
        }
    }

    #[test]
    fn versions_share_a_project() {
        for name in [
            "robot_v03.mb",
            "robot-v4.mb",
            "robot v2 final.mb",
            "robot_final.mb",
            "robot (1).mb",
            "Robot_V012.mb",
            "robot.mb",
        ] {
            assert_eq!(project_group(name).to_lowercase(), "robot.mb", "{name}");
        }
        // Shot and asset numbers are not versions.
        assert_eq!(project_group("shot_010.nk"), "shot_010.nk");
        assert_eq!(project_group("shot_010_v004.nk"), "shot_010.nk");
        assert_eq!(project_group("level2.blend"), "level2.blend");
        assert_eq!(project_group("v2.blend"), "v2.blend");
        assert_eq!(project_group("dev.blend"), "dev.blend");
        assert_eq!(project_group("rev2.blend"), "rev2.blend");
    }
}
