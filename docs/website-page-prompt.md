# Prompt: "Ounce" page for the ef tools website

Paste the whole block below into your site builder (Claude, Lovable, Framer AI, v0…) together with the images from `docs/press/`.

---

Create a dedicated page for my software **Ounce** on my personal website **ef tools** (by Enzo Freminet). The page must match the existing site exactly: dark background, **Manrope** typeface, a minimal, premium look inspired by Apple product pages. Generous whitespace, large tight headlines (weight 800, negative letter-spacing), grey secondary text (#a1a1a6), very dark grey cards (#1c1c1e) with large rounded corners (24 px) and no border. One accent only, an iridescent soap-film gradient (linear-gradient(115deg, #5EE7FF, #8F8BFF 32%, #FF6AD5 64%, #FFD36E), the colours of the logo), used sparingly: the key figures, the primary highlight, a thin underline. The app icon (`logo-512.png`) is a soap bubble on a black rounded tile: show it as is, never recoloured. Apart from that accent, no gradients, no neon, no emoji in headings. All copy is in English.

## Animations
Smooth and understated, triggered on scroll:
- elements rise in with a fade (translateY 24 px → 0, blur 6 px → 0, 700 ms, easing cubic-bezier(.22,1,.36,1)), staggered by 80 ms;
- the main screenshot scales up slightly as it enters the viewport (scale .96 → 1);
- key figures count up to their value;
- pill-shaped buttons that compress slightly when pressed.

Respect `prefers-reduced-motion` (no animation at all). Fully responsive, flawless on mobile (16 px side margins, no horizontal scrolling).

## Provided images
- `01-hero.png`: main visual (16:9)
- `02-features.png`: feature grid
- `03-featherweight.png`: key figures
- `capture-today.png`, `capture-week.png`, `capture-wrapped.png`, `capture-year.png`, `capture-apps.png`: interface screenshots (2880×1800)
- `wrapped-share.png`: the Wrapped share card (4:5 portrait)
- `export-minimal.png`, `export-receipt-photo.png`, `export-arcade-cabinet.png`: project share images (4:5 portrait)
- `og-1200x630.png`: use as the page's Open Graph / Twitter image

Show the screenshots inside a sober window frame (dark grey title bar with three grey dots, rounded corners, very soft shadow). Use `loading="lazy"` except for the top image, and descriptive alt text on every image.

## Structure and copy

**1. Hero**
- Eyebrow: "Ounce · for Windows 10/11"
- Headline: "Screen time that weighs an ounce."
- Subheadline: "A 2 MB screen-time tracker for Windows. Featherweight, private, built for 3D artists."
- Primary white button: "Download for free" → `https://github.com/bruce333-creator/ounce/releases/latest/download/ounce.exe`
- Secondary link: "View source code" → `https://github.com/bruce333-creator/ounce`
- Small grey line under the button: "Free · ~2 MB · Windows 10 and 11"
- Then `capture-today.png`, large, in the window frame.

**2. "Everything that matters. Nothing more."**: a grid of 6 cards:
- **Real screen time**: "Only the time you actually use your PC counts. Breaks, sleep and a locked screen are left out."
- **A Wrapped every week**: "Your profile of the week (Marathoner, Night owl, Zen…), your top 5 and your records, shareable as one image."
- **Time per project**: "Maya, Blender, Houdini, Photoshop… Time spent on each scene is detected automatically."
- **Wellbeing**: "Break reminders, a daily goal and per-app limits, as Windows notifications."
- **Your year**: "A calendar where every day is a square, with your creative streaks and records."
- **Share a project**: "Turn one project into an image: time, days, sessions, breaks, earliest start and latest finish. Minimal, receipt (flat or crumpled for real), arcade screen or arcade cabinet."

**3. Wrapped section**: text on the left, `wrapped-share.png` on the right, slightly tilted (rotate -3°), straightening on hover.
- Headline: "Your week, told."
- Text: "Every Monday, Ounce prepares your Wrapped: how long you spent on your PC, your #1 app, your busiest day and the profile that fits you."

**4. Gallery**: a horizontal carousel or "Today / Week / Year / Apps" tabs that switch the screenshot with a crossfade.

**5. "Weighs nothing. Misses nothing."**: the core message of the page. 4 key figures that count up when they appear, very large:
- "~2 MB": one .exe, no installer, no dependencies
- "~16 MB": of memory while running
- "2 s": a quick look every 2 seconds, then back to sleep
- "0": telemetry. Your data stays on your PC
Under them, small grey text: "Measured on Windows by the build's selftest."

**6. Get started in 3 steps** (numbers 1-2-3 in circles):
1. Download `ounce.exe`.
2. Double-click it: Ounce installs itself (no admin rights, no installer), appears in the Start menu, and its icon shows up next to the Windows clock.
3. Right-click the icon → "Run at Windows startup". That's it.

**7. FAQ**: animated accordion (smooth height, rotating chevron):
- *Is my data sent anywhere?* "No. Everything is stored on your PC. The only connection is a daily update check with GitHub, carrying no personal data. You can turn it off in the settings."
- *Will it slow my PC down?* "No. It looks at the foreground app every 2 seconds, then goes back to sleep at low priority. It never records keystrokes or your screen."
- *Windows says "Windows protected your PC".* "That's normal for new software that isn't code-signed. Click "More info", then "Run anyway". It only happens on the first install: updates install without it."
- *How do I update?* "A notification tells you when a new version is out: one click and it installs itself, keeping all your data."
- *Mac or Linux?* "Windows 10 and 11 only, for now."
- *Is it really free?* "Yes, and open source (MIT license)."

**8. Final call to action**: a large centred headline "Ready to see where your time goes?" + "Download for free" button (same link).

**9. Footer**, small grey text: "Ounce by Enzo Freminet · ef tools · MIT license"

## Technical
- Meta tags: title "Ounce: screen time that weighs an ounce | ef tools", a description of at most 155 characters, Open Graph with `og-1200x630.png`.
- No heavy animation library: IntersectionObserver + CSS are enough.
- Accessible contrast, visible keyboard focus.
- Leave the rest of the site unchanged: only add this page and an "Ounce" link in the menu, next to my other tools.
