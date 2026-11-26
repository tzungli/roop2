// ============================================================
// Shared genre classification (used by renderer app.js and main.js)
// Single source of truth to keep library scanning and UI grouping consistent.
// ============================================================
'use strict';

const CANONICAL_GENRES = [
  'Classical', 'Jazz', 'Rock', 'Pop', 'Electronic',
  'Soul / R&B', 'Blues', 'Folk', 'Soundtrack', 'Metal',
  'Hip Hop', 'Ambient', 'Vocal', 'Audiophile'
];

const CANONICAL_GENRE_SET = new Set(CANONICAL_GENRES.map(g => g.toLowerCase()));

function matchGenreRules(text, { strictMeta = false } = {}) {
  if (!text) return null;

  // 1. Audiophile demo / exhibition / test discs (explicit demo pressings only — never random FLAC/Hi-Res)
  if (/(?:\baudiophile\b|\bbinaural\b|burmester|bowers\s*&\s*wilkins.*society|chesky\s*records?\s*sampler|stockfisch|sheffield\s*(?:lab|jazz)|reference\s*recordings|opus\s*3|telarc\s*sampler|hi-?end.*(?:cd|音響)|音響大展|音響展|展cd|示範片|測試片|終極測試|發燒試音|試音碟|發燒天碟|試音\s*蔡琴)/i.test(text)) {
    return 'Audiophile';
  }

  // 2. Soundtrack / Anime / Game / Musical (strict word boundaries — never match "Ghost" for "ost")
  if (/(?:\bsoundtrack\b|\bost\b|\bo\.s\.t\.?\b|\bfilm\s*score\b|\bmovie\s*score\b|\bcinematic\b|\bgame\s*music\b|\banime\b|\bvocaloid\b|bandes\s+originales|tvアニメ|アニメ|サウンドトラック|イメージ・ソング|オープニングテーマ|エンディングテーマ|ゲーム|動畫音樂|動畫影視音樂|原聲帶|原聲|配樂|影視|動漫|音樂劇|highlights\s+from|sunset\s+boulevard|jekyll\s*&\s*hyde|zombie\s*land\s*saga|ゾンビランドサガ|franchouchou|death\s*musume|myth\s*&\s*roid|君の名は|天気の子|となりのトトロ|仙劍奇俠傳|イース|symphony\s+ys|final\s*fantasy|studio\s*ghibli|久石譲|久石讓|澤野弘之|梶浦由記|菅野よう子|hans\s*zimmer|john\s*williams|ennio\s*morricone)/i.test(text)) {
    return 'Soundtrack';
  }

  // 3. Classical (avoid bare "piano" or "樂團" which hijack pop/rock bands like F.I.R. 飛兒樂團)
  if (/(?:\bclassical\b|\borchestra(?:l)?\b|\bsymphon(?:y|ic)\b|\bbaroque\b|\bopera\b|\bchamber\s*music\b|\bphilharmonic\b|\bconcerto\b|\bsonata\b|\brequiem\b|\bnocturne\b|\bstring\s*quartet\b|\bcello\s*suite\b|\bmozart\b|\bbeethoven\b|\bbach\b|\bchopin\b|\btchaikovsky\b|\bbrahms\b|\bhandel\b|\bvivaldi\b|\bhaydn\b|\bschubert\b|\bschumann\b|\bmahler\b|\bdebussy\b|\bravel\b|\brachmaninoff\b|\bprokofiev\b|\bshostakovich\b|\borff\b|carmina\s+burana|古典|交響曲|交響樂|協奏曲|奏鳴曲|管弦樂|室內樂|歌劇|弦樂四重奏|愛樂管弦|明尼蘇達管弦)/i.test(text)) {
    return 'Classical';
  }

  // 4. Jazz
  if (/(?:\bjazz\b|\bjazz\s*fusion\b|\bswing\b|\bbebop\b|\bhard\s*bop\b|\bbig\s*band\b|\bdixieland\b|\bsmooth\s*jazz\b|\bbossa\s*nova\b|\bblue\s*bossa\b|miles\s*davis|john\s*coltrane|bill\s*evans|chet\s*baker|diana\s*krall|keith\s*jarrett|oscar\s*peterson|dave\s*brubeck|herbie\s*hancock|stan\s*getz|ella\s*fitzgerald|louis\s*armstrong|pat\s*metheny|norah\s*jones|jacintha|stacey\s*kent|anne\s*bisson|爵士|巴薩諾瓦)/i.test(text)) {
    return 'Jazz';
  }

  // 5. Metal
  if (/(?:\bheavy\s*metal\b|\bthrash\s*metal\b|\bdeath\s*metal\b|\bblack\s*metal\b|\bdoom\s*metal\b|\bpower\s*metal\b|\bprog(?:ressive)?\s*metal\b|\bmetalcore\b|\bnu[\s-]?metal\b|重金屬|金屬樂)/i.test(text) || (!strictMeta && /\bmetal\b/i.test(text))) {
    return 'Metal';
  }

  // 6. Blues
  if (/(?:\bblues\b|\bdelta\s*blues\b|\bchicago\s*blues\b|\bcountry\s*blues\b|\belectric\s*blues\b|b\.?\s*b\.?\s*king|stevie\s*ray\s*vaughan|muddy\s*waters|robert\s*johnson|藍調)/i.test(text)) {
    return 'Blues';
  }

  // 7. Soul / R&B
  if (/(?:\br&b\b|\brnb\b|\brhythm\s+and\s+blues\b|\bmotown\b|\bneo-?soul\b|\bfunk\b|\bgospel\b|marvin\s*gaye|stevie\s*wonder|aretha\s*franklin|alicia\s*keys|amy\s*winehouse|lisa\s*stansfield|war\s*-\s*deliver|節奏藍調|靈魂樂)/i.test(text) || (!strictMeta && /\bsoul\b/i.test(text))) {
    return 'Soul / R&B';
  }

  // 8. Hip Hop (strict word boundaries — never match "shadowgraph" or "trapdoor")
  if (/(?:\bhip[\s-]?hop\b|\brap\b|\btrap\s*music\b|\bboom\s*bap\b|kendrick\s*lamar|kanye\s*west|eminem|jay-?z|notorious\s*b\.?i\.?g|tupac|嘻哈|饒舌)/i.test(text)) {
    return 'Hip Hop';
  }

  // 9. Rock
  if (/(?:\brock\b|\bpunk\b|\bgrunge\b|\balternative\b|\bindie\s*rock\b|\bhard\s*rock\b|\bclassic\s*rock\b|\bpsychedelic\b|\bprog(?:ressive)?\s*rock\b|\bbritpop\b|\bj-?rock\b|\bpost-?rock\b|coldplay|pink\s*floyd|led\s*zeppelin|the\s*beatles|rolling\s*stones|\bqueen\b|radiohead|nirvana|dire\s*straits|\beagles\b|fleetwood\s*mac|deep\s*purple|ac\/dc|\boasis\b|\bmuse\b|linkin\s*park|tears\s*for\s*fears|radwimps|king\s*gnu|back\s*number|one\s*ok\s*rock|五月天|伍佰|搖滾|朋克)/i.test(text)) {
    return 'Rock';
  }

  // 10. Folk
  if (/(?:\bfolk\b|\bbluegrass\b|\bceltic\b|\bamericana\b|\bcountry\b|\bsinger-?songwriter\b|joan\s*(?:c\.?\s*)?baez|bob\s*dylan|simon\s*&\s*garfunkel|joni\s*mitchell|nick\s*drake|james\s*taylor|leonard\s*cohen|indigo\s*girls|richard\s*&\s*linda\s*thompson|许巍|許巍|民謠|民歌|民樂|鄉村)/i.test(text)) {
    return 'Folk';
  }

  // 11. Electronic (never match bare "dance" inside "Tiny Dancers" or "Sun Dance")
  if (/(?:\belectronic\b|\belectronica\b|\btechno\b|\bdeep\s*house\b|\bhouse\s*music\b|\btrance\b|\bsynthwave\b|\bsynth-?pop\b|\bedm\b|\bdowntempo\b|\bchillout\b|\bdrum\s*(?:&|and)\s*bass\b|\bdubstep\b|\bdisco\b|daft\s*punk|kraftwerk|aphex\s*twin|massive\s*attack|portishead|chemical\s*brothers|電子|電音)/i.test(text)) {
    return 'Electronic';
  }

  // 12. Ambient / New Age
  if (/(?:\bambient\b|\bnew\s*age\b|\bmeditation\b|\bdrone\b|\batmospheric\b|contemporary\s+instrumental|\benya\b|vangelis|yanni|喜多郎|朱哲琴|阿姐鼓|新世紀|氛圍|冥想)/i.test(text)) {
    return 'Ambient';
  }

  // 13. Vocal / Audiophile Vocal / Chanson
  if (/(?:\bvocal\s*jazz\b|\baudiophile\s*vocal\b|\bacapella\b|\ba\s*cappella\b|\bchoral\b|\bchanson\b|susan\s*wong|amanda\s*mcbroom|eva\s*cassidy|emi\s*fujita|藤田惠美|蔡琴|陳潔麗|趙鵬|童麗|王聞|玉置浩二|發燒人聲|美聲|聲樂|人聲)/i.test(text) || (!strictMeta && /\bvocal\b/i.test(text))) {
    return 'Vocal';
  }

  // 14. Pop (Mandopop / Cantopop / J-Pop / K-Pop / Western Pop)
  if (/(?:\bpop\b|\bcantopop\b|\bmandopop\b|\bc-?pop\b|\bj-?pop\b|\bk-?pop\b|\bballad\b|asian\s*music|\baimer\b|\baiko\b|\bdaoko\b|\breol\b|\bmajiko\b|まじ娘|浜崎あゆみ|熊木杏里|西村知美|garnidelia|\begoist\b|\bjunna\b|hélène|helene|\badele\b|carole\s*king|周杰倫|張學友|王菲|陳奕迅|林俊傑|鄧麗君|黃舒駿|shu\s*jun\s*huang|飛兒樂團|f\.?i\.?r|自然捲|老歌|流行|華語|一般流行)/i.test(text)) {
    return 'Pop';
  }

  return null;
}

function canonicalizeGenre(rawGenre, track = {}) {
  const g = String(rawGenre || '').trim();
  const trackRaw = String(track.rawGenre || '').trim();
  const explicitRawTag = (trackRaw && !CANONICAL_GENRE_SET.has(trackRaw.toLowerCase()))
    ? trackRaw
    : (g && !CANONICAL_GENRE_SET.has(g.toLowerCase()) ? g : '');

  // 1. Trust explicit non-canonical file genre tags first (e.g. "Mandopop", "Bossa Nova", "Progressive Rock")
  if (explicitRawTag) {
    const fromRawTag = matchGenreRules(explicitRawTag, { strictMeta: false });
    if (fromRawTag) return fromRawTag;
  }

  // 2. Infer from album title + artist ONLY (never scan filePath!) with strict word boundaries
  const metaText = [track.album || track.title || '', track.albumArtist || '', track.artist || ''].join(' ').trim();
  const fromMeta = matchGenreRules(metaText, { strictMeta: true });
  if (fromMeta) return fromMeta;

  // 3. If the existing genre is already a valid canonical genre and no stronger metadata signal disagreed, keep it
  if (g && CANONICAL_GENRE_SET.has(g.toLowerCase())) {
    const exact = CANONICAL_GENRES.find(c => c.toLowerCase() === g.toLowerCase());
    if (exact) return exact;
  }

  // 4. Fallback: check rawGenre again loosely, else default to Pop
  if (g) {
    const loose = matchGenreRules(g, { strictMeta: false });
    if (loose) return loose;
  }

  return 'Pop';
}

// Renderer (classic script tag) exposure
if (typeof window !== 'undefined') {
  window.CANONICAL_GENRES = CANONICAL_GENRES;
  window.canonicalizeGenre = canonicalizeGenre;
}

// Node.js (main process) exposure
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { CANONICAL_GENRES, canonicalizeGenre };
}
