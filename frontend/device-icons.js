// device-icons.js — Hardware-modeled monochrome line-art SVG library for Hi-Fi streamers & wireless speakers
(function (globalScope) {
  'use strict';

  const HARDWARE_PROFILES = [
    {
      id: 'eversolo_streamer',
      label: 'Eversolo Streamer',
      match: /\b(eversolo|dmp-?a6|dmp-?a8|dmp-?a10|dmp-?a\d*|zidoo)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3" y="8" width="38" height="16" rx="1.8" fill="currentColor" fill-opacity="0.04"/>
        <line x1="6.5" y1="24" x2="6.5" y2="25.5" stroke-width="1.5"/>
        <line x1="37.5" y1="24" x2="37.5" y2="25.5" stroke-width="1.5"/>
        <rect x="6.5" y="10.5" width="21" height="11" rx="1.2" fill="currentColor" fill-opacity="0.07"/>
        <rect x="8.2" y="12.2" width="5.5" height="7.5" rx="0.6" stroke-width="0.85" opacity="0.7"/>
        <line x1="15.5" y1="14" x2="25" y2="14" stroke-width="0.95" opacity="0.8"/>
        <line x1="15.5" y1="16.8" x2="22.5" y2="16.8" stroke-width="0.8" opacity="0.55"/>
        <circle cx="34.5" cy="16" r="4.1" stroke-width="1.05"/>
        <circle cx="34.5" cy="16" r="2.2" fill="currentColor" fill-opacity="0.1"/>
        <line x1="4.4" y1="11.5" x2="4.4" y2="20.5" stroke-width="0.8" opacity="0.45"/>
      </svg>`
    },
    {
      id: 'hifirose',
      label: 'HiFi Rose',
      match: /\b(hifi\s*rose|hifirose|rs150\w*|rs130\w*|rs250\w*|rs520\w*|rs201\w*)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3" y="7.5" width="38" height="17" rx="1.6" fill="currentColor" fill-opacity="0.04"/>
        <rect x="5" y="9.5" width="34" height="13" rx="0.9" fill="currentColor" fill-opacity="0.07"/>
        <rect x="7" y="11.5" width="13.5" height="9" rx="0.8" stroke-width="0.85" opacity="0.75"/>
        <rect x="23.5" y="11.5" width="13.5" height="9" rx="0.8" stroke-width="0.85" opacity="0.75"/>
        <path d="M 9.2 18 A 5.2 5.2 0 0 1 18.3 18" stroke-width="0.8" opacity="0.65"/>
        <line x1="13.7" y1="19" x2="15.8" y2="14.2" stroke-width="0.9"/>
        <path d="M 25.7 18 A 5.2 5.2 0 0 1 34.8 18" stroke-width="0.8" opacity="0.65"/>
        <line x1="30.2" y1="19" x2="32.3" y2="14.2" stroke-width="0.9"/>
        <line x1="7" y1="24.5" x2="7" y2="26" stroke-width="1.5"/>
        <line x1="37" y1="24.5" x2="37" y2="26" stroke-width="1.5"/>
      </svg>`
    },
    {
      id: 'wiim_ultra',
      label: 'WiiM Ultra',
      match: /\bwiim\s*ultra\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
        <rect x="6" y="8" width="32" height="16" rx="2.6" fill="currentColor" fill-opacity="0.05"/>
        <rect x="9" y="10.8" width="11.5" height="10.4" rx="1.2" fill="currentColor" fill-opacity="0.08"/>
        <line x1="11.5" y1="14.2" x2="18" y2="14.2" stroke-width="0.9" opacity="0.75"/>
        <line x1="11.5" y1="17" x2="16" y2="17" stroke-width="0.8" opacity="0.5"/>
        <circle cx="24" cy="18.2" r="1.1" fill="currentColor" stroke="none" opacity="0.65"/>
        <circle cx="31.5" cy="16" r="3.8" fill="currentColor" fill-opacity="0.08"/>
        <line x1="31.5" y1="16" x2="33.6" y2="14" stroke-width="1"/>
      </svg>`
    },
    {
      id: 'wiim_puck',
      label: 'WiiM / LinkPlay Streamer',
      match: /\b(wiim|linkplay)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M 7 14 Q 22 9.5 37 14 L 37 21 Q 22 25.5 7 21 Z" fill="currentColor" fill-opacity="0.05"/>
        <ellipse cx="22" cy="13.5" rx="15" ry="4.2" fill="currentColor" fill-opacity="0.07"/>
        <circle cx="15" cy="21" r="0.8" fill="currentColor" stroke="none" opacity="0.65"/>
        <circle cx="18.5" cy="21.4" r="0.8" fill="currentColor" stroke="none" opacity="0.65"/>
        <circle cx="22" cy="21.6" r="0.9" fill="currentColor" stroke="none"/>
        <circle cx="25.5" cy="21.4" r="0.8" fill="currentColor" stroke="none" opacity="0.65"/>
      </svg>`
    },
    {
      id: 'naim_muso',
      label: 'Naim Mu-so',
      match: /\b(mu-?so)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <path d="M 4 12 L 40 12 L 40 21.5 L 4 21.5 Z" fill="currentColor" fill-opacity="0.05"/>
        <rect x="6" y="21.5" width="32" height="3" rx="0.5" opacity="0.55"/>
        <ellipse cx="32.5" cy="11.2" rx="3.4" ry="1.4" fill="currentColor" fill-opacity="0.1"/>
        <path d="M 7 15.5 Q 14 14 22 15.5 T 37 15.5" stroke-width="0.85" opacity="0.65"/>
        <path d="M 7 18.2 Q 14 16.7 22 18.2 T 37 18.2" stroke-width="0.85" opacity="0.65"/>
      </svg>`
    },
    {
      id: 'naim_uniti',
      label: 'Naim Uniti / Streamer',
      match: /\b(naim|uniti\s*(atom|star|nova)|nd5\w*|ndx\w*|nd555)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <polygon points="8,11.5 22,6.5 36,11.5 22,16.5" fill="currentColor" fill-opacity="0.06"/>
        <polygon points="8,11.5 22,16.5 22,25.5 8,20.5" fill="currentColor" fill-opacity="0.04"/>
        <polygon points="22,16.5 36,11.5 36,20.5 22,25.5" fill="currentColor" fill-opacity="0.08"/>
        <ellipse cx="22" cy="11.4" rx="4.4" ry="2.1" fill="currentColor" fill-opacity="0.12"/>
        <polygon points="10.5,14.4 18.5,17.2 18.5,22 10.5,19.2" stroke-width="0.85" fill="currentColor" fill-opacity="0.08"/>
        <line x1="25" y1="18.2" x2="25" y2="23.2" stroke-width="0.85" opacity="0.6"/>
        <line x1="27.8" y1="17.2" x2="27.8" y2="22.2" stroke-width="0.85" opacity="0.6"/>
        <line x1="30.6" y1="16.2" x2="30.6" y2="21.2" stroke-width="0.85" opacity="0.6"/>
        <line x1="33.4" y1="15.2" x2="33.4" y2="20.2" stroke-width="0.85" opacity="0.6"/>
      </svg>`
    },
    {
      id: 'cambridge_audio',
      label: 'Cambridge Audio',
      match: /\b(cambridge|cxn\w*|mxn\d*|axn\d*|evo\s*(?:75|150)|edge\s*nq)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <rect x="4" y="9" width="36" height="14" rx="1.2" fill="currentColor" fill-opacity="0.05"/>
        <rect x="7" y="23" width="30" height="2" rx="0.5" fill="currentColor" fill-opacity="0.12"/>
        <rect x="13" y="12" width="15" height="8" rx="0.8" fill="currentColor" fill-opacity="0.08"/>
        <circle cx="34" cy="16" r="3" fill="currentColor" fill-opacity="0.08"/>
        <circle cx="8" cy="16" r="1" fill="currentColor" stroke="none" opacity="0.65"/>
        <line x1="10.2" y1="14.5" x2="10.2" y2="17.5" stroke-width="0.9" opacity="0.65"/>
      </svg>`
    },
    {
      id: 'bluesound_node',
      label: 'Bluesound / NAD BluOS',
      match: /\b(bluesound|bluos|powernode|node\s*(?:icon|x|2i?|n\d+)?|nad\s*(?:m10|m33|m66|c\s*\d+))\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <polygon points="8,12 36,12 39,17 5,17" fill="currentColor" fill-opacity="0.07"/>
        <rect x="5" y="17" width="34" height="6.5" rx="1" fill="currentColor" fill-opacity="0.04"/>
        <line x1="14" y1="14.5" x2="30" y2="14.5" stroke-width="0.9" opacity="0.7"/>
        <circle cx="22" cy="20.2" r="1.1" fill="currentColor" stroke="none"/>
      </svg>`
    },
    {
      id: 'lumin',
      label: 'LUMIN Network Player',
      match: /\blumin\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <path d="M 3 11 Q 22 8.5 41 11 L 41 13.5 L 3 13.5 Z" fill="currentColor" fill-opacity="0.08"/>
        <rect x="5" y="13.5" width="34" height="10" rx="1" fill="currentColor" fill-opacity="0.04"/>
        <rect x="13" y="16" width="18" height="4.5" rx="0.7" fill="currentColor" fill-opacity="0.1"/>
        <line x1="15.5" y1="18.2" x2="28.5" y2="18.2" stroke-width="0.85" opacity="0.75"/>
      </svg>`
    },
    {
      id: 'aurender',
      label: 'Aurender Music Server',
      match: /\baurender\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3.5" y="8.5" width="37" height="15" rx="1.2" fill="currentColor" fill-opacity="0.05"/>
        <circle cx="8" cy="16" r="1.8" stroke-width="1"/>
        <rect x="12.5" y="11" width="19" height="10" rx="0.8" fill="currentColor" fill-opacity="0.08"/>
        <circle cx="34.8" cy="14" r="0.8" fill="currentColor" stroke="none" opacity="0.75"/>
        <circle cx="37.4" cy="14" r="0.8" fill="currentColor" stroke="none" opacity="0.75"/>
        <circle cx="34.8" cy="18" r="0.8" fill="currentColor" stroke="none" opacity="0.75"/>
        <circle cx="37.4" cy="18" r="0.8" fill="currentColor" stroke="none" opacity="0.75"/>
        <line x1="6.5" y1="23.5" x2="6.5" y2="25.2" stroke-width="1.5"/>
        <line x1="37.5" y1="23.5" x2="37.5" y2="25.2" stroke-width="1.5"/>
      </svg>`
    },
    {
      id: 'linn_dsm',
      label: 'Linn DSM',
      match: /\b(linn|klimax|selekt|majik|akurate)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <rect x="4" y="10" width="36" height="13" rx="1.4" fill="currentColor" fill-opacity="0.05"/>
        <line x1="10" y1="16.5" x2="17" y2="16.5" stroke-width="0.95" opacity="0.65"/>
        <circle cx="22" cy="16.5" r="4.2" fill="currentColor" fill-opacity="0.09"/>
        <circle cx="22" cy="16.5" r="2.1" stroke-width="0.9" opacity="0.8"/>
        <line x1="27" y1="16.5" x2="34" y2="16.5" stroke-width="0.95" opacity="0.65"/>
      </svg>`
    },
    {
      id: 'marantz',
      label: 'Marantz Hi-Fi',
      match: /\bmarantz\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <rect x="4" y="7.5" width="36" height="17" rx="1.4" fill="currentColor" fill-opacity="0.05"/>
        <line x1="11" y1="7.5" x2="11" y2="24.5" stroke-width="0.8" opacity="0.4"/>
        <line x1="33" y1="7.5" x2="33" y2="24.5" stroke-width="0.8" opacity="0.4"/>
        <circle cx="8" cy="16" r="2.2" fill="currentColor" fill-opacity="0.08"/>
        <circle cx="22" cy="16" r="4.3" fill="currentColor" fill-opacity="0.1"/>
        <circle cx="22" cy="16" r="2.7" stroke-width="0.85" opacity="0.7"/>
        <circle cx="36" cy="16" r="2.2" fill="currentColor" fill-opacity="0.08"/>
      </svg>`
    },
    {
      id: 'mcintosh',
      label: 'McIntosh',
      match: /\bmcintosh\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <rect x="4" y="6" width="36" height="20" rx="1.4" fill="currentColor" fill-opacity="0.06"/>
        <rect x="8.5" y="8.8" width="11.5" height="8" rx="0.9" fill="currentColor" fill-opacity="0.1"/>
        <rect x="24" y="8.8" width="11.5" height="8" rx="0.9" fill="currentColor" fill-opacity="0.1"/>
        <path d="M 10.2 15.2 A 4.8 4.8 0 0 1 18.3 15.2" stroke-width="0.8" opacity="0.65"/>
        <line x1="14.2" y1="15.8" x2="16.2" y2="11" stroke-width="0.9"/>
        <path d="M 25.7 15.2 A 4.8 4.8 0 0 1 33.8 15.2" stroke-width="0.8" opacity="0.65"/>
        <line x1="29.7" y1="15.8" x2="31.7" y2="11" stroke-width="0.9"/>
        <circle cx="10.5" cy="21.5" r="2.1"/>
        <circle cx="33.5" cy="21.5" r="2.1"/>
        <line x1="17" y1="21.5" x2="27" y2="21.5" stroke-width="0.9" opacity="0.65"/>
      </svg>`
    },
    {
      id: 'fiio_desktop',
      label: 'Desktop Hi-Fi Streamer / DAC',
      match: /\b(fiio|shanling|hiby|cayin|aune|smsl|topping|matrix\s*audio|chord|ifi|neo\s*stream|zen\s*stream)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <rect x="13" y="3.5" width="18" height="25" rx="2" fill="currentColor" fill-opacity="0.05"/>
        <rect x="15.2" y="5.8" width="13.6" height="12.5" rx="1" fill="currentColor" fill-opacity="0.09"/>
        <circle cx="17.2" cy="23.2" r="1.4" stroke-width="0.95"/>
        <circle cx="21.5" cy="23.2" r="1" fill="currentColor" stroke="none" opacity="0.75"/>
        <circle cx="26.2" cy="23.2" r="2.2" fill="currentColor" fill-opacity="0.08"/>
      </svg>`
    },
    {
      id: 'av_receiver',
      label: 'Hi-Fi Network Receiver',
      match: /\b(denon|heos|yamaha|musiccast|onkyo|pioneer|integra|arcam|rotel|sony|teac|esoteric|luxman|accuphase|simaudio|moon)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <rect x="4" y="7" width="36" height="18" rx="1.3" fill="currentColor" fill-opacity="0.05"/>
        <rect x="13" y="10" width="18" height="7" rx="0.8" fill="currentColor" fill-opacity="0.09"/>
        <circle cx="8" cy="16" r="2.5" fill="currentColor" fill-opacity="0.08"/>
        <circle cx="36" cy="16" r="2.5" fill="currentColor" fill-opacity="0.08"/>
        <line x1="14" y1="20.5" x2="30" y2="20.5" stroke-width="0.9" opacity="0.6"/>
        <line x1="7" y1="25" x2="7" y2="26.5" stroke-width="1.5"/>
        <line x1="37" y1="25" x2="37" y2="26.5" stroke-width="1.5"/>
      </svg>`
    },
    {
      id: 'kef_uniq',
      label: 'KEF Wireless Speaker',
      match: /\b(kef|ls50\w*|lsx\w*|ls60\w*)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
        <rect x="13" y="4" width="18" height="24" rx="2.6" fill="currentColor" fill-opacity="0.05"/>
        <circle cx="22" cy="16" r="6.4" fill="currentColor" fill-opacity="0.07"/>
        <circle cx="22" cy="16" r="3.1" stroke-width="0.95"/>
        <circle cx="22" cy="16" r="1.2" fill="currentColor" stroke="none"/>
        <line x1="22" y1="10.2" x2="22" y2="12.5" stroke-width="0.75" opacity="0.6"/>
        <line x1="22" y1="19.5" x2="22" y2="21.8" stroke-width="0.75" opacity="0.6"/>
        <line x1="16.2" y1="16" x2="18.5" y2="16" stroke-width="0.75" opacity="0.6"/>
        <line x1="25.5" y1="16" x2="27.8" y2="16" stroke-width="0.75" opacity="0.6"/>
      </svg>`
    },
    {
      id: 'devialet_phantom',
      label: 'Devialet Phantom',
      match: /\b(devialet|phantom)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
        <rect x="10" y="7" width="24" height="18" rx="9" fill="currentColor" fill-opacity="0.06"/>
        <path d="M 10.5 11.5 A 6.5 5.5 0 0 0 10.5 20.5" stroke-width="1.15"/>
        <path d="M 33.5 11.5 A 6.5 5.5 0 0 1 33.5 20.5" stroke-width="1.15"/>
        <circle cx="22" cy="16" r="3.4" fill="currentColor" fill-opacity="0.1"/>
        <circle cx="22" cy="16" r="1.3" fill="currentColor" stroke="none"/>
      </svg>`
    },
    {
      id: 'bo_beosound',
      label: 'Bang & Olufsen Beosound',
      match: /\b(bang\s*&\s*olufsen|bang\s+and\s+olufsen|b&o|beosound|beolab|beoplay)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="22" cy="13.5" r="9.5" fill="currentColor" fill-opacity="0.06"/>
        <circle cx="22" cy="13.5" r="7.5" stroke-width="0.8" opacity="0.55"/>
        <line x1="16" y1="21" x2="12" y2="28.5" stroke-width="1.25"/>
        <line x1="22" y1="23" x2="22" y2="28.5" stroke-width="1.25"/>
        <line x1="28" y1="21" x2="32" y2="28.5" stroke-width="1.25"/>
      </svg>`
    },
    {
      id: 'bw_zeppelin',
      label: 'Bowers & Wilkins Zeppelin',
      match: /\b(zeppelin|bowers|b&w|formation)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
        <ellipse cx="22" cy="14" rx="18" ry="6.2" fill="currentColor" fill-opacity="0.06"/>
        <line x1="12" y1="14" x2="32" y2="14" stroke-width="0.8" opacity="0.45"/>
        <path d="M 16.5 20 L 14.5 25 L 29.5 25 L 27.5 20" stroke-width="1.15"/>
        <line x1="13" y1="25" x2="31" y2="25" stroke-width="1.4"/>
      </svg>`
    },
    {
      id: 'sonos_era',
      label: 'Sonos Wireless Speaker',
      match: /\b(sonos|era\s*100|era\s*300|play:\d|sonos\s*(?:one|five|arc|beam|port|amp))\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M 13 6.5 L 31 6.5 C 29 12.5 29 19.5 31 25.5 L 13 25.5 C 15 19.5 15 12.5 13 6.5 Z" fill="currentColor" fill-opacity="0.06"/>
        <line x1="15.5" y1="11.5" x2="28.5" y2="11.5" stroke-width="0.8" opacity="0.5"/>
        <line x1="16" y1="16" x2="28" y2="16" stroke-width="0.8" opacity="0.5"/>
        <line x1="15.5" y1="20.5" x2="28.5" y2="20.5" stroke-width="0.8" opacity="0.5"/>
      </svg>`
    },
    {
      id: 'apple_homepod',
      label: 'Apple HomePod',
      match: /\bhomepod\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
        <rect x="14.5" y="6.5" width="15" height="19" rx="6" fill="currentColor" fill-opacity="0.06"/>
        <ellipse cx="22" cy="8.5" rx="5.2" ry="1.8" fill="currentColor" fill-opacity="0.1"/>
        <circle cx="22" cy="8.5" r="0.9" fill="currentColor" stroke="none" opacity="0.85"/>
        <line x1="16.5" y1="14" x2="27.5" y2="14" stroke-width="0.75" opacity="0.35"/>
        <line x1="16.5" y1="17.5" x2="27.5" y2="17.5" stroke-width="0.75" opacity="0.35"/>
        <line x1="17" y1="21" x2="27" y2="21" stroke-width="0.75" opacity="0.35"/>
      </svg>`
    },
    {
      id: 'apple_tv',
      label: 'Apple TV',
      match: /\b(apple\s*tv|appletv)\b/i,
      svg: (w, h) => `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M 8 13.5 Q 22 9 36 13.5 L 36 20 Q 22 24.5 8 20 Z" fill="currentColor" fill-opacity="0.05"/>
        <ellipse cx="22" cy="13.5" rx="14" ry="3.8" fill="currentColor" fill-opacity="0.08"/>
        <circle cx="28.5" cy="20.2" r="0.8" fill="currentColor" stroke="none" opacity="0.8"/>
      </svg>`
    }
  ];

  function buildSearchString(dev) {
    if (!dev || typeof dev === 'string') return '';
    return [dev.manufacturer, dev.model, dev.name].filter(Boolean).join(' ');
  }

  function matchDeviceProfile(dev) {
    if (!dev || dev === 'local' || dev.isLocal) return null;
    const text = buildSearchString(dev);
    if (!text) return null;
    for (const profile of HARDWARE_PROFILES) {
      if (profile.match.test(text)) {
        return profile;
      }
    }
    return null;
  }

  function getCategoryFallbackKind(devOrKind) {
    if (typeof devOrKind === 'string') return devOrKind;
    if (!devOrKind || devOrKind.isLocal) return 'speaker';
    if (devOrKind.protocol === 'airplay' || devOrKind.deviceCategory === 'airplay' || String(devOrKind.id || '').startsWith('airplay:')) {
      return 'airplay';
    }
    const name = String(devOrKind.name || '').toLowerCase();
    if (devOrKind.deviceCategory === 'tv' || name.includes('webos') || name.includes('tv')) return 'tv';
    if (devOrKind.deviceCategory === 'bridge' || name.includes('hqplayer') || name.includes('squeezebox')) return 'bridge';
    return 'streamer';
  }

  function getCategorySvg(kind, isBottomBar) {
    const w = isBottomBar ? 34 : 44;
    const h = isBottomBar ? 28 : 34;

    if (kind === 'streamer') {
      return `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
        <polygon points="4,12.5 23.5,5.5 40,12 20.5,19.5" fill="currentColor" fill-opacity="0.04"/>
        <polygon points="4,12.5 20.5,19.5 20.5,26 4,18.6"/>
        <polygon points="20.5,19.5 40,12 40,18.2 20.5,26" fill="currentColor" fill-opacity="0.06"/>
        <line x1="4.4" y1="14.4" x2="20.1" y2="21.4" stroke-width="0.85"/>
        <line x1="4.4" y1="16.4" x2="20.1" y2="23.5" stroke-width="0.85"/>
        <line x1="22.8" y1="22.1" x2="37.6" y2="16.4" stroke-width="0.9" opacity="0.55"/>
      </svg>`;
    }

    if (kind === 'bridge') {
      return `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round">
        <path d="M 3 19 L 11 19 L 13 16 L 15 21 L 17.5 11 L 20 25 L 22.5 6 L 25 23 L 27.5 17 L 30 19 L 41 19"/>
        <path d="M 27 24.5 V 8.5 C 31.5 10 34 13 33.5 17" stroke-width="1.4"/>
        <ellipse cx="24.8" cy="24.8" rx="3.1" ry="2.2" transform="rotate(-18 24.8 24.8)" fill="currentColor"/>
      </svg>`;
    }

    if (kind === 'airplay') {
      return `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M 14.5 21.5 A 9.5 9.5 0 1 1 29.5 21.5"/>
        <path d="M 17.5 18.5 A 5.5 5.5 0 1 1 26.5 18.5"/>
        <polygon points="22,19 16.5,27 27.5,27" fill="currentColor" stroke="none"/>
      </svg>`;
    }

    if (kind === 'tv') {
      return `<svg width="${w}" height="${h}" viewBox="0 0 44 32" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
        <rect x="7" y="5" width="30" height="19" rx="2"/>
        <line x1="16" y1="27.5" x2="28" y2="27.5"/>
        <line x1="22" y1="24" x2="22" y2="27.5"/>
      </svg>`;
    }

    const sw = isBottomBar ? 26 : 30;
    const sh = isBottomBar ? 28 : 32;
    return `<svg width="${sw}" height="${sh}" viewBox="0 0 32 36" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
      <rect x="7" y="3" width="18" height="30" rx="2.2"/>
      <circle cx="16" cy="11.2" r="2.7"/>
      <circle cx="16" cy="22.8" r="5.3"/>
      <circle cx="16" cy="22.8" r="1.4" fill="currentColor" stroke="none"/>
    </svg>`;
  }

  function getDeviceSvg(devOrKind, isBottomBar = false) {
    const w = isBottomBar ? 34 : 44;
    const h = isBottomBar ? 28 : 34;
    if (devOrKind && typeof devOrKind === 'object') {
      const profile = matchDeviceProfile(devOrKind);
      if (profile) {
        return profile.svg(w, h);
      }
    }
    return getCategorySvg(getCategoryFallbackKind(devOrKind), isBottomBar);
  }

  globalScope.ROO_DEVICE_ICONS = {
    profiles: HARDWARE_PROFILES,
    matchDeviceProfile,
    getDeviceSvg
  };
})(typeof window !== 'undefined' ? window : globalThis);
