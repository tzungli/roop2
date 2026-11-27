// ============================================================
// Shared Roon-aligned 21 Music Genres classification
// Single source of truth for library scanning, reclassification, and UI grouping.
// ============================================================
'use strict';

const CANONICAL_GENRES = [
  'Pop/Rock',
  'Classical',
  'Jazz',
  'R&B',
  'Stage & Screen',
  'International',
  'Vocal',
  'Country',
  'Folk',
  'Latin',
  'Blues',
  'New Age',
  'Easy Listening',
  'Holiday',
  'Electronic',
  "Children's",
  'Rap',
  'Avant-Garde',
  'Religious',
  'Reggae',
  'Comedy/Spoken'
];

const ROON_GENRE_ZH_MAP = {
  'Pop/Rock': '流行 / 搖滾',
  'Classical': '古典樂',
  'Jazz': '爵士樂',
  'R&B': '節奏藍調',
  'Stage & Screen': '舞台與影視',
  'International': '國際音樂',
  'Vocal': '人聲',
  'Country': '鄉村音樂',
  'Folk': '民謠',
  'Latin': '拉丁音樂',
  'Blues': '藍調',
  'New Age': '新世紀音樂',
  'Easy Listening': '輕音樂',
  'Holiday': '節日音樂',
  'Electronic': '電子音樂',
  "Children's": '兒童音樂',
  'Rap': '饒舌',
  'Avant-Garde': '前衛音樂',
  'Religious': '宗教',
  'Reggae': '雷鬼',
  'Comedy/Spoken': '喜劇 / 口說藝術'
};

const CANONICAL_GENRE_SET = new Set(CANONICAL_GENRES.map(g => g.toLowerCase()));

// Legacy genre names from older rooPlayer builds that should be re-evaluated cleanly
const LEGACY_GENRE_MAP = {
  'pop': 'Pop/Rock',
  'rock': 'Pop/Rock',
  'metal': 'Pop/Rock',
  'soundtrack': 'Stage & Screen',
  'soul / r&b': 'R&B',
  'ambient': 'New Age',
  'hip hop': 'Rap',
  'audiophile': 'Easy Listening'
};

function matchRoonGenreRules(text, { strictMeta = false } = {}) {
  if (!text) return null;

  // 1. Holiday / Christmas
  if (/(?:\bchristmas\b|\bholiday\b|\bxmas\b|\bcarols?\b|聖誕|耶誕|節日音樂|新年音樂會)/i.test(text)) {
    return 'Holiday';
  }

  // 2. Children's
  if (/(?:\bchildren'?s\b|\bkids?\s*music\b|\blullaby\b|\blullabies\b|\bnursery\b|兒童音樂|兒歌|童謠)/i.test(text)) {
    return "Children's";
  }

  // 3. Comedy / Spoken Word
  if (/(?:\bcomedy\b|\bspoken\s*word\b|\baudiobook\b|\bstand-?up\b|喜劇|口說|相聲|朗誦|有聲書)/i.test(text)) {
    return 'Comedy/Spoken';
  }

  // 4. Religious / Sacred / Gospel
  if (/(?:\breligious\b|\bgospel\b|\bsacred\b|\bchristian\b|\bworship\b|\bhymns?\b|\bchant\b|\bgregorian\b|宗教|聖歌|福音|佛樂|禪樂)/i.test(text)) {
    return 'Religious';
  }

  // 5. Stage & Screen (Soundtrack / Musical / Anime / Game)
  if (/(?:\bstage\s*&\s*screen\b|\bsoundtrack\b|\bost\b|\bo\.s\.t\.?\b|\bfilm\s*score\b|\bmovie\s*score\b|\bcinematic\b|\bmusical\b|\bbroadway\b|\bgame\s*music\b|\banime\b|\bvocaloid\b|bandes\s+originales|tvアニメ|アニメ|サウンドトラック|イメージ・ソング|オープニングテーマ|エンディングテーマ|ゲーム|動畫音樂|動畫影視音樂|原聲帶|原聲|配樂|影視|動漫|舞台|音樂劇|highlights\s+from|sunset\s+boulevard|jekyll\s*&\s*hyde|zombie\s*land\s*saga|ゾンビランドサガ|franchouchou|death\s*musume|myth\s*&\s*roid|君の名は|天気の子|となりのトトロ|仙劍奇俠傳|イース|symphony\s+ys|final\s*fantasy|studio\s*ghibli|久石譲|久石讓|澤野弘之|梶浦由記|菅野よう子|hans\s*zimmer|john\s*williams|ennio\s*morricone)/i.test(text)) {
    return 'Stage & Screen';
  }

  // 6. Classical
  if (/(?:\bclassical\b|\borchestra(?:l)?\b|\bsymphon(?:y|ic)\b|\bbaroque\b|\bopera\b|\bchamber\s*music\b|\bphilharmonic\b|\bconcerto\b|\bsonata\b|\brequiem\b|\bnocturne\b|\bstring\s*quartet\b|\bcello\s*suite\b|\bmozart\b|\bbeethoven\b|\bbach\b|\bchopin\b|\btchaikovsky\b|\bbrahms\b|\bhandel\b|\bvivaldi\b|\bhaydn\b|\bschubert\b|\bschumann\b|\bmahler\b|\bdebussy\b|\bravel\b|\brachmaninoff\b|\bprokofiev\b|\bshostakovich\b|\borff\b|carmina\s+burana|古典|交響曲|交響樂|協奏曲|奏鳴曲|管弦樂|室內樂|歌劇|弦樂四重奏|愛樂管弦|明尼蘇達管弦)/i.test(text)) {
    return 'Classical';
  }

  // 7. Jazz
  if (/(?:\bjazz\b|\bjazz\s*fusion\b|\bswing\b|\bbebop\b|\bhard\s*bop\b|\bbig\s*band\b|\bdixieland\b|\bsmooth\s*jazz\b|\bbossa\s*nova\b|\bblue\s*bossa\b|sheffield\s*jazz|miles\s*davis|john\s*coltrane|bill\s*evans|chet\s*baker|diana\s*krall|keith\s*jarrett|oscar\s*peterson|dave\s*brubeck|herbie\s*hancock|stan\s*getz|ella\s*fitzgerald|louis\s*armstrong|pat\s*metheny|norah\s*jones|jacintha|stacey\s*kent|anne\s*bisson|爵士|巴薩諾瓦)/i.test(text)) {
    return 'Jazz';
  }

  // 8. Latin
  if (/(?:\blatin\b|\bsalsa\b|\btango\b|\bflamenco\b|\bsamba\b|\bmambo\b|\bbolero\b|\bbachata\b|\breggaeton\b|astor\s*piazzolla|buena\s*vista\s*social\s*club|拉丁|探戈|佛朗明哥)/i.test(text)) {
    return 'Latin';
  }

  // 9. Reggae
  if (/(?:\breggae\b|\bdub\b|\bska\b|\bdancehall\b|bob\s*marley|雷鬼)/i.test(text)) {
    return 'Reggae';
  }

  // 10. Blues
  if (/(?:\bblues\b|\bdelta\s*blues\b|\bchicago\s*blues\b|\belectric\s*blues\b|b\.?\s*b\.?\s*king|stevie\s*ray\s*vaughan|muddy\s*waters|robert\s*johnson|藍調)/i.test(text)) {
    return 'Blues';
  }

  // 11. R&B / Soul / Funk
  if (/(?:\br&b\b|\brnb\b|\brhythm\s+and\s+blues\b|\bmotown\b|\bneo-?soul\b|\bfunk\b|marvin\s*gaye|stevie\s*wonder|aretha\s*franklin|alicia\s*keys|amy\s*winehouse|lisa\s*stansfield|節奏藍調|靈魂樂)/i.test(text) || (!strictMeta && /\bsoul\b/i.test(text))) {
    return 'R&B';
  }

  // 12. Rap / Hip-Hop
  if (/(?:\bhip[\s-]?hop\b|\brap\b|\btrap\s*music\b|\bboom\s*bap\b|kendrick\s*lamar|kanye\s*west|eminem|jay-?z|notorious\s*b\.?i\.?g|tupac|嘻哈|饒舌)/i.test(text)) {
    return 'Rap';
  }

  // 13. Country
  if (/(?:\bcountry\b|\bbluegrass\b|\bnashville\b|\bamericana\b|dolly\s*parton|johnny\s*cash|willie\s*nelson|garth\s*brooks|鄉村)/i.test(text)) {
    return 'Country';
  }

  // 14. Folk
  if (/(?:\bfolk\b|\bsinger-?songwriter\b|joan\s*(?:c\.?\s*)?baez|bob\s*dylan|simon\s*&\s*garfunkel|joni\s*mitchell|nick\s*drake|james\s*taylor|leonard\s*cohen|indigo\s*girls|richard\s*&\s*linda\s*thompson|许巍|許巍|民謠|民歌)/i.test(text)) {
    return 'Folk';
  }

  // 15. International / World / Ethnic / Traditional
  if (/(?:\binternational\b|\bworld\s*music\b|\bceltic\b|\bafrobeat\b|\bchanson\b|\bfado\b|\bgamelan\b|\btaiko\b|\benka\b|演歌|鬼太鼓座|ondekoza|馬頭琴|民樂|國樂|古箏|二胡|琵琶|笛子|國際音樂|世界音樂|民族音樂|阿姐鼓|朱哲琴|hélène|helene)/i.test(text) || (!strictMeta && /\bworld\b/i.test(text))) {
    return 'International';
  }

  // 16. Avant-Garde / Experimental
  if (/(?:\bavant-?garde\b|\bexperimental\b|\bnoise\b|\bminimalism\b|\bfree\s*jazz\b|\bmusique\s*concr|前衛|實驗音樂)/i.test(text)) {
    return 'Avant-Garde';
  }

  // 17. New Age / Ambient
  if (/(?:\bnew\s*age\b|\bambient\b|\bmeditation\b|\bdrone\b|\batmospheric\b|contemporary\s+instrumental|\benya\b|vangelis|yanni|喜多郎|坂本龍一|新世紀|氛圍|冥想)/i.test(text)) {
    return 'New Age';
  }

  // 18. Electronic
  if (/(?:\belectronic\b|\belectronica\b|\btechno\b|\bdeep\s*house\b|\bhouse\s*music\b|\btrance\b|\bsynthwave\b|\bsynth-?pop\b|\bedm\b|\bdowntempo\b|\bchillout\b|\bdrum\s*(?:&|and)\s*bass\b|\bdubstep\b|\bdisco\b|daft\s*punk|kraftwerk|aphex\s*twin|massive\s*attack|portishead|chemical\s*brothers|電子|電音)/i.test(text) || (!strictMeta && /\bdance\b/i.test(text))) {
    return 'Electronic';
  }

  // 19. Vocal (Audiophile Vocal / Standards / Acapella)
  if (/(?:\bvocal\s*jazz\b|\baudiophile\s*vocal\b|\bacapella\b|\ba\s*cappella\b|\bchoral\b|susan\s*wong|amanda\s*mcbroom|eva\s*cassidy|emi\s*fujita|藤田惠美|蔡琴|陳潔麗|趙鵬|童麗|王聞|玉置浩二|tony\s*bennett|frank\s*sinatra|發燒人聲|人聲|美聲|聲樂)/i.test(text) || (!strictMeta && /\bvocal\b/i.test(text))) {
    return 'Vocal';
  }

  // 20. Easy Listening (Lounge / Instrumental / Audiophile Demo & Test CDs)
  if (/(?:\beasy\s*listening\b|\blounge\b|\bexotica\b|\bmuzak\b|\binstrumental\s*pop\b|\baudiophile\b|\bbinaural\b|burmester|bowers\s*&\s*wilkins.*society|chesky\s*records?\s*sampler|stockfisch|sheffield\s*lab|reference\s*recordings|opus\s*3|telarc\s*sampler|hi-?end.*(?:cd|音響)|音響大展|音響展|展cd|示範片|測試片|終極測試|發燒試音|試音碟|發燒天碟|輕音樂|抒情演奏)/i.test(text) || (!strictMeta && /\binstrumental\b/i.test(text))) {
    return 'Easy Listening';
  }

  // 21. Pop/Rock (Pop, Rock, Metal, Mandopop, Cantopop, J-Pop, K-Pop)
  if (/(?:\bpop\b|\brock\b|\bmetal\b|\bpunk\b|\bgrunge\b|\balternative\b|\bindie\b|\bhard\s*rock\b|\bclassic\s*rock\b|\bpsychedelic\b|\bprog(?:ressive)?\s*rock\b|\bbritpop\b|\bj-?rock\b|\bj-?pop\b|\bc-?pop\b|\bk-?pop\b|\bhk-?pop\b|\bcantopop\b|\bmandopop\b|\bkayokyoku\b|\bballad\b|asian\s*music|coldplay|pink\s*floyd|led\s*zeppelin|the\s*beatles|rolling\s*stones|\bqueen\b|radiohead|nirvana|dire\s*straits|\beagles\b|fleetwood\s*mac|deep\s*purple|ac\/dc|\boasis\b|\bmuse\b|linkin\s*park|tears\s*for\s*fears|radwimps|king\s*gnu|back\s*number|one\s*ok\s*rock|\baimer\b|\baiko\b|\bdaoko\b|\breol\b|\bmajiko\b|まじ娘|浜崎あゆみ|熊木杏里|西村知美|garnidelia|\begoist\b|\bjunna\b|\badele\b|carole\s*king|周杰倫|張學友|王菲|陳奕迅|林俊傑|鄧麗君|黃舒駿|shu\s*jun\s*huang|飛兒樂團|f\.?i\.?r|自然捲|五月天|伍佰|老歌|流行|搖滾|重金屬|華語|一般流行)/i.test(text)) {
    return 'Pop/Rock';
  }

  return null;
}

function canonicalizeGenre(rawGenre, track = {}) {
  const g = String(rawGenre || '').trim();
  const trackRaw = String(track.rawGenre || '').trim();
  const onlineRaw = String(track.onlineGenre || '').trim();
  const gLower = g.toLowerCase();
  const trackRawLower = trackRaw.toLowerCase();

  // Identify genuinely specific file genre tag (neither already Roon-canonical nor an old overwritten legacy/unknown bucket)
  const UNKNOWN_PLACEHOLDERS = new Set(['', 'unknown', 'various', 'other', 'music', 'none', 'test']);
  const isGenericOrLegacy = (s) => UNKNOWN_PLACEHOLDERS.has(s) || CANONICAL_GENRE_SET.has(s) || Object.prototype.hasOwnProperty.call(LEGACY_GENRE_MAP, s);
  const explicitRawTag = !isGenericOrLegacy(trackRawLower)
    ? trackRaw
    : (!isGenericOrLegacy(gLower) ? g : '');

  // 1. Priority 1: Explicit non-canonical file genre tag wins first
  if (explicitRawTag) {
    const fromRaw = matchRoonGenreRules(explicitRawTag, { strictMeta: false });
    if (fromRaw) return fromRaw;
  }

  // 1b. Priority 1 (continued): Already-valid specific Roon canonical tag from file metadata (non-Pop/Rock default)
  const isSpecificCanonicalFileTag = g && CANONICAL_GENRE_SET.has(gLower) && gLower !== 'pop/rock';
  if (isSpecificCanonicalFileTag) {
    const exact = CANONICAL_GENRES.find(c => c.toLowerCase() === gLower);
    if (exact) return exact;
  }

  // 2. Priority 2: Online structured metadata (iTunes Search API -> MusicBrainz Release-Group API)
  if (onlineRaw) {
    const onlineLower = onlineRaw.toLowerCase();
    if (CANONICAL_GENRE_SET.has(onlineLower)) {
      const exact = CANONICAL_GENRES.find(c => c.toLowerCase() === onlineLower);
      if (exact) return exact;
    }
    const fromOnline = matchRoonGenreRules(onlineRaw, { strictMeta: false });
    if (fromOnline) return fromOnline;
  }

  // 3. Priority 3: Infer from album title + artist ONLY (never scan filePath) with strict word boundaries
  const metaText = [track.album || track.title || '', track.albumArtist || '', track.artist || ''].join(' ').trim();
  const fromMeta = matchRoonGenreRules(metaText, { strictMeta: true });
  if (fromMeta) return fromMeta;

  // 4. Keep already-valid Roon canonical genre (including Pop/Rock) if no stronger signal disagreed
  if (g && CANONICAL_GENRE_SET.has(gLower)) {
    const exact = CANONICAL_GENRES.find(c => c.toLowerCase() === gLower);
    if (exact) return exact;
  }

  // 5. Map legacy canonical genre cleanly
  if (g && Object.prototype.hasOwnProperty.call(LEGACY_GENRE_MAP, gLower)) {
    return LEGACY_GENRE_MAP[gLower];
  }

  // 6. Loose fallback on rawGenre
  if (g) {
    const loose = matchRoonGenreRules(g, { strictMeta: false });
    if (loose) return loose;
  }

  return 'Pop/Rock';
}

function getRoonGenreZhName(genreName) {
  if (!genreName) return '音樂類型';
  if (ROON_GENRE_ZH_MAP[genreName]) return ROON_GENRE_ZH_MAP[genreName];
  const canon = canonicalizeGenre(genreName);
  return ROON_GENRE_ZH_MAP[canon] || genreName;
}

// Renderer (classic script tag) exposure
if (typeof window !== 'undefined') {
  window.CANONICAL_GENRES = CANONICAL_GENRES;
  window.ROON_GENRE_ZH_MAP = ROON_GENRE_ZH_MAP;
  window.canonicalizeGenre = canonicalizeGenre;
  window.getRoonGenreZhName = getRoonGenreZhName;
}

// Node.js (main process / CLI scripts) exposure
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { CANONICAL_GENRES, ROON_GENRE_ZH_MAP, canonicalizeGenre, getRoonGenreZhName };
}
