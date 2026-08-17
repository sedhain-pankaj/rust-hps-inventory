const ICONS = {
  "back-arrow": '<path d="M19 12H5"/><path d="M12 19l-7-7 7-7"/>',
  fingerprint: '<path d="M12 10a2 2 0 0 0-2 2c0 1.02-.07 2.03-.2 3.03"/><path d="M14 13.12c0 2.38 0 6.38-1 8.88"/><path d="M17.29 21.02c.12-.6.43-2.3.5-3.02"/><path d="M2 12a10 10 0 0 1 18-6"/><path d="M2 16h.01"/><path d="M21.8 16c.2-2 .13-5.35 0-6"/><path d="M5 19.5C5.5 18 6 15 6 12a6 6 0 0 1 .34-2"/><path d="M8.65 22c.21-.66.45-1.32.57-2"/><path d="M9 6.8a6 6 0 0 1 9 5.2v2"/><path d="M14.83 17.09c.63 1.53 1.07 3.19 1.26 4.91"/><path d="M17.69 11a10 10 0 0 1 1.22 4.51"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  user: '<circle cx="12" cy="8" r="4"/><path d="M5 21c0-3.9 3.1-6 7-6s7 2.1 7 6"/>',
  users: '<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20c0-3.6 2.9-5.5 6.5-5.5s6.5 1.9 6.5 5.5"/><path d="M16 4.7a3.5 3.5 0 0 1 0 6.6"/><path d="M18.8 15.2c1.7.9 2.7 2.4 2.7 4.3"/>',
  shield: '<path d="M12 3l7 3v5.5c0 4.3-2.9 7.7-7 9.5-4.1-1.8-7-5.2-7-9.5V6z"/>',
  book: '<path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20"/>',
  box: '<path d="M21 8.5l-9-5-9 5v7l9 5 9-5z"/><path d="M3 8.5l9 5 9-5"/><path d="M12 13.5V21"/>',
  layers: '<path d="M12 2L2 7l10 5 10-5z"/><path d="M2 12l10 5 10-5"/><path d="M2 17l10 5 10-5"/>',
  truck: '<path d="M2 6h12v10H2z"/><path d="M14 10h4l3 3v3h-7"/><circle cx="6.5" cy="18" r="2"/><circle cx="17.5" cy="18" r="2"/>',
  gauge: '<path d="M4.5 18.5a9.5 9.5 0 1 1 15 0"/><path d="M12 14l3.5-5"/>',
  database: '<ellipse cx="12" cy="5.5" rx="8" ry="3"/><path d="M4 5.5v13c0 1.66 3.58 3 8 3s8-1.34 8-3v-13"/><path d="M4 12c0 1.66 3.58 3 8 3s8-1.34 8-3"/>',
  bell: '<path d="M6 9a6 6 0 0 1 12 0c0 5 2 6.5 2 6.5H4S6 14 6 9"/><path d="M10.3 19.5a2 2 0 0 0 3.4 0"/>',
  "alert-triangle": '<path d="M12 3.5L2.5 19.5h19z"/><path d="M12 9.5v4.5"/><path d="M12 17.2v.3"/>',
  dollar: '<path d="M12 2.5v19"/><path d="M16.5 6.8c-1.1-1.3-2.8-2-4.7-2-2.6 0-4.6 1.4-4.6 3.6 0 4.6 9.4 2.4 9.4 7 0 2.3-2.1 3.7-4.8 3.7-2 0-3.8-.7-4.9-2"/>',
  list: '<path d="M8.5 6h12"/><path d="M8.5 12h12"/><path d="M8.5 18h12"/><path d="M4 6h.01"/><path d="M4 12h.01"/><path d="M4 18h.01"/>',
  check: '<path d="M4.5 12.5l5 5L19.5 6.5"/>',
  x: '<path d="M6 6l12 12"/><path d="M18 6L6 18"/>',
  plus: '<path d="M12 5v14"/><path d="M5 12h14"/>',
  refresh: '<path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/>',
};

export function icon(name, size = 24) {
  const paths = ICONS[name] || "";
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;
}

export const ICON_NAMES = Object.keys(ICONS);
