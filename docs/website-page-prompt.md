# Prompt: update the "Activity Tracker" page of ef tools to "Ounce"

Paste the block below into your site builder, together with the images listed in it (all in `docs/press/`).

---

Update the existing **Activity Tracker** page of my site **ef tools**. The software is now called **Ounce**. Keep the site's look exactly as it is (dark background, Manrope, Apple-like spacing, cards #1c1c1e with 24 px corners, grey secondary text #a1a1a6). Accent colour: violet #BF5AF2, used sparingly. English only.

## 1. Rename everywhere
- "Activity Tracker" → **Ounce** in the page title, headings, menu entry, alt texts, meta tags and the URL slug (`/ounce`, with a redirect from the old URL if the builder allows it).
- Logo: `logo-512.png` (a soap bubble on a black rounded tile). Show it as is, never recoloured.

## 2. Download link
Replace every old GitHub link with:
- Download button → `https://github.com/bruce333-creator/ounce/releases/latest/download/ounce.exe`
- "Source code" link → `https://github.com/bruce333-creator/ounce`
Under the button, in small grey text: "Free · Windows 10 and 11 · 2.4 MB · installs itself, no admin rights".

## 3. Cut the text
Remove long paragraphs, the "how time is counted" explanations, repeated claims and anything technical (config files, command lines, registry). One headline and at most one short sentence per section. Let the images speak.

## 4. New page structure (in this order)

**Hero**
- Eyebrow: "Ounce · for Windows"
- Headline: "Screen time that weighs an ounce."
- One line: "A 2 MB tracker built for 3D artists."
- Download button + "Source code" link.
- Image: `capture-today.png` in a simple window frame.

**Features**: a grid of 6 short cards (title + one line each, no more):
1. **Time per project**: "Maya, Blender, Houdini… every scene is timed, versions grouped."
2. **Render time**: "Away while it renders? Counted apart."
3. **Focus mode**: "25 or 50 minutes, reminders on hold."
4. **Weekly Wrapped**: "Your week as one shareable image."
5. **Share a project**: "Turn a project into a receipt or an arcade screen." Show `export-receipt-photo.png` and `export-arcade-cabinet.png` side by side under the grid, slightly tilted.
6. **Taskbar widget**: "Today's time, next to the clock."

**Themes** (new section, big and visual)
- Headline: "Five themes. Your taskbar follows."
- One line: "Dark, Light, Terminal, Blueprint and Viewport, inspired by the tools you already use."
- Image: `themes.png` full width. If the builder supports it, add five small pill buttons (Dark / Light / Terminal / Blueprint / Viewport) that crossfade between `theme-default.png`, `theme-light.png`, `theme-terminal.png`, `theme-blueprint.png`, `theme-viewport.png`.

**Featherweight**: 3 big numbers in a row that count up on scroll: "2.4 MB" (one exe), "16 MB" (memory), "0" (telemetry). No sentences under them, just the label.

**Get started**: 3 steps in one row: "Download" · "Double-click: it installs itself" · "Pick your theme".

**FAQ**: 4 short questions in an accordion:
- *Is my data sent anywhere?* "No. Everything stays on your PC."
- *Windows shows a warning.* "Click "More info" → "Run anyway". Only the first time."
- *How do updates work?* "Ounce checks every 6 hours and installs in one click."
- *Is it free?* "Yes, and open source (MIT)."

**Final call to action**: "Ready to see where your time goes?" + the same Download button.

## 5. Images to use (from `docs/press/`)
`logo-512.png`, `capture-today.png`, `capture-settings.png` (optional, in the Themes section), `themes.png`, `theme-*.png`, `export-receipt-photo.png`, `export-arcade-cabinet.png`, `og-1200x630.png` (Open Graph). Use `loading="lazy"` except for the hero image, and short alt texts.

## 6. Keep
Smooth scroll-in animations (fade + slight rise), `prefers-reduced-motion` respected, perfect on mobile (16 px side margins). Page title: "Ounce: screen time that weighs an ounce | ef tools".
