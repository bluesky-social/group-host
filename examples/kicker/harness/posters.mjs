// Posters for the seeded events, as a riso print of them would look: two
// inks overprinted on paper, a halftone, grain, and the event's name set
// large. SVG, so they are made here rather than kept as files, and
// uploaded to each host's own PDS as any picture would be.

/** Riso inks, as they are sold. */
const INKS = {
  pink: '#ff48b0',
  blue: '#0078bf',
  yellow: '#ffe800',
  teal: '#00838a',
  orange: '#ff6c2f'
};

/** A number from `text`, the same each time, to vary a poster by. */
const hash = (text) => [...text].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7);

const escape = (text) =>
  text.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

const FONT = 'Helvetica Neue, Helvetica, Arial, sans-serif';

/**
 * A poster for `name`, 800 by 1000, in the inks `first` and `second`
 * (names of INKS): a halftone band and a square in the second, a disc in
 * the first, overprinting where they meet, and the name in the first, a
 * word a line, all with the ink missing in specks, as a drum leaves it.
 */
export function poster(name, first, second) {
  const h = hash(name);
  const [a, b] = [INKS[first], INKS[second]];
  const cx = 280 + (h % 240);
  const cy = 240 + ((h >> 8) % 80);
  const r = 150 + ((h >> 16) % 60);
  const turn = (h % 40) - 20;
  const words = name.toLowerCase().split(' ');
  const top = 960 - 120 - (words.length - 1) * 92;
  const lines = words
    .map((w, i) => `<text x="56" y="${top + i * 92}">${escape(w)}</text>`)
    .join('');
  const multiply = 'style="mix-blend-mode:multiply"';
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 1000" width="800" height="1000">
<defs>
  <pattern id="dots" width="14" height="14" patternUnits="userSpaceOnUse" patternTransform="rotate(${turn})">
    <circle cx="7" cy="7" r="4.2" fill="${b}"/>
  </pattern>
  <filter id="grain">
    <feTurbulence type="fractalNoise" baseFrequency="0.8" numOctaves="2" seed="${h % 97}" result="noise"/>
    <feColorMatrix in="noise" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 -2.5 2.2" result="ink"/>
    <feComposite in="SourceGraphic" in2="ink" operator="in"/>
  </filter>
</defs>
<rect width="800" height="1000" fill="#f4f1ea"/>
<g filter="url(#grain)">
  <rect x="${cx - 400}" y="${cy - 30}" width="800" height="${r}" fill="url(#dots)" transform="rotate(${turn} ${cx} ${cy})"/>
  <circle cx="${cx}" cy="${cy}" r="${r}" fill="${a}" ${multiply}/>
  <rect x="${cx - r / 2}" y="${cy - r * 1.1}" width="${r * 1.2}" height="${r * 1.2}" fill="${b}" ${multiply} transform="rotate(${-turn} ${cx} ${cy})"/>
  <g font-family="${FONT}" font-weight="700" font-size="104" letter-spacing="-4" fill="${a}" ${multiply}>${lines}</g>
  <text x="58" y="950" font-family="${FONT}" font-size="26" fill="${b}">kicker</text>
</g>
</svg>
`;
}

/**
 * A picture for a member's card, 400 square, in the inks `first` and
 * `second`: a disc in the first and a square in the second, overprinting,
 * placed by `name`, as the same member's is wherever it is drawn.
 */
export function portrait(name, first, second) {
  const h = hash(name);
  const [a, b] = [INKS[first], INKS[second]];
  const turn = (h % 40) - 20;
  const multiply = 'style="mix-blend-mode:multiply"';
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 400" width="400" height="400">
<rect width="400" height="400" fill="#f4f1ea"/>
<circle cx="${170 + (h % 60)}" cy="${170 + ((h >> 8) % 60)}" r="130" fill="${a}" ${multiply}/>
<rect x="${150 + ((h >> 4) % 70)}" y="${130 + ((h >> 12) % 70)}" width="160" height="160" fill="${b}" ${multiply} transform="rotate(${turn} 200 200)"/>
</svg>
`;
}
