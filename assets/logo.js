// Draws the Ounce icon (a soap bubble on a black rounded tile) at size N.
function drawIcon(c, N, opts = {}) {
  const g = c.getContext('2d'), u = N / 64, small = N <= 24, paused = !!opts.paused;
  const cols = paused ? ['#d0d0d5', '#8e8e93', '#c7c7cc', '#6e6e73', '#e5e5ea', '#d0d0d5']
                      : ['#5ee7ff', '#6f8bff', '#c86bff', '#ff5fd2', '#ffd36e', '#7dffb3', '#5ee7ff'];
  const conic = (cx, cy, a0) => { const k = g.createConicGradient(a0, cx, cy); cols.forEach((col, i) => k.addColorStop(i / (cols.length - 1), col)); return k; };
  g.clearRect(0, 0, N, N);
  // Tile.
  const R = 14.5 * u;
  g.save(); g.beginPath(); g.roundRect(0, 0, N, N, R); g.clip();
  const bg = g.createLinearGradient(0, 0, 0, N); bg.addColorStop(0, '#1b1b20'); bg.addColorStop(1, '#000'); g.fillStyle = bg; g.fillRect(0, 0, N, N);
  const sw = g.createRadialGradient(N * .16, -N * .04, 0, N * .16, -N * .04, N * .85);
  sw.addColorStop(0, 'rgba(255,255,255,.26)'); sw.addColorStop(.45, 'rgba(255,255,255,.06)'); sw.addColorStop(1, 'rgba(255,255,255,0)');
  g.fillStyle = sw; g.fillRect(0, 0, N, N);
  const cx = 32 * u, cy = 33 * u, r = (small ? 22 : 21) * u;
  if (small) {
    g.lineWidth = 3.4 * u; g.strokeStyle = conic(cx, cy, -0.6); g.beginPath(); g.arc(cx, cy, r - 1.7 * u, 0, Math.PI * 2); g.stroke();
    g.fillStyle = '#fff'; g.beginPath(); g.arc(cx + r * .42, cy - r * .42, 3.2 * u, 0, Math.PI * 2); g.fill();
  } else {
    // Faint glow around the film.
    g.save(); g.filter = `blur(${2.2 * u}px)`; g.globalAlpha = .35; g.lineWidth = 2 * u; g.strokeStyle = conic(cx, cy, -0.6);
    g.beginPath(); g.arc(cx, cy, r, 0, Math.PI * 2); g.stroke(); g.restore();
    // Iridescent film: strong at the edge, clear in the middle.
    const film = document.createElement('canvas'); film.width = film.height = N; const f = film.getContext('2d');
    f.fillStyle = (() => { const k = f.createConicGradient(-0.6, cx, cy); cols.forEach((col, i) => k.addColorStop(i / (cols.length - 1), col)); return k; })();
    f.beginPath(); f.arc(cx, cy, r, 0, Math.PI * 2); f.fill();
    f.globalCompositeOperation = 'destination-in';
    const m = f.createRadialGradient(cx, cy, r * .55, cx, cy, r);
    m.addColorStop(0, 'rgba(0,0,0,0)'); m.addColorStop(.6, 'rgba(0,0,0,.18)'); m.addColorStop(.92, 'rgba(0,0,0,.75)'); m.addColorStop(1, 'rgba(0,0,0,1)');
    f.fillStyle = m; f.fillRect(0, 0, N, N);
    g.drawImage(film, 0, 0);
    // Swirl of colour inside the upper right, like light on soap.
    g.save(); g.beginPath(); g.arc(cx, cy, r, 0, Math.PI * 2); g.clip();
    g.filter = `blur(${1.6 * u}px)`; g.globalCompositeOperation = 'screen'; g.globalAlpha = .85;
    g.lineWidth = 4.2 * u; g.strokeStyle = conic(cx + 2 * u, cy - 2 * u, 3.6);
    g.beginPath(); g.arc(cx - 3 * u, cy + 3 * u, r * .98, -1.45, -0.05); g.stroke();
    g.globalAlpha = .55; g.lineWidth = 3 * u; g.beginPath(); g.arc(cx + 3 * u, cy - 2 * u, r * .9, 1.85, 2.9); g.stroke();
    g.restore();
    // Thin bright rim.
    g.lineWidth = .55 * u; g.globalAlpha = .6; g.strokeStyle = conic(cx, cy, 1.2); g.beginPath(); g.arc(cx, cy, r - .35 * u, 0, Math.PI * 2); g.stroke(); g.globalAlpha = 1;
    // Highlights: a sharp sparkle upper right, a soft window reflection lower left.
    const spot = (x, y, rad, a) => { const h = g.createRadialGradient(x, y, 0, x, y, rad); h.addColorStop(0, `rgba(255,255,255,${a})`); h.addColorStop(.35, `rgba(255,255,255,${a * .5})`); h.addColorStop(1, 'rgba(255,255,255,0)'); g.fillStyle = h; g.beginPath(); g.arc(x, y, rad, 0, Math.PI * 2); g.fill(); };
    spot(cx + r * .6, cy - r * .5, 4.2 * u, 1);
    g.save(); g.translate(cx - r * .62, cy + r * .5); g.rotate(-0.75); g.scale(1, .45); spot(0, 0, 5.5 * u, .8); g.restore();
    g.save(); g.translate(cx - r * .45, cy - r * .62); g.rotate(-0.6); g.scale(1, .35); spot(0, 0, 4 * u, .35); g.restore();
  }
  g.restore();
  // Edge of the tile.
  const e = g.createLinearGradient(0, 0, 0, N); e.addColorStop(0, 'rgba(255,255,255,.28)'); e.addColorStop(.3, 'rgba(255,255,255,.06)'); e.addColorStop(1, 'rgba(255,255,255,.1)');
  g.strokeStyle = e; g.lineWidth = Math.max(1, .6 * u); g.beginPath(); g.roundRect(.3 * u, .3 * u, N - .6 * u, N - .6 * u, R - .3 * u); g.stroke();
}
