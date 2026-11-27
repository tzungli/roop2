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

const ROON_GENRE_DESCRIPTIONS = {
  'Pop/Rock': '流行與搖滾是最具包容力且不斷演化的當代音樂語彙。從早期融合鄉村與藍調的能量、直率反叛與洗腦旋律出發，隨著時代推演，這個龐大譜系涵蓋了經典搖滾的吉他張力、披頭四式的精緻和聲、華語與日韓流行的細膩抒情，乃至於獨立搖滾、英倫搖滾與重金屬的前衛探索，既重視精雕細琢的歌曲工藝，也持續拓展當代聲響的邊界。',
  'Classical': '古典樂橫跨巴洛克、古典主義、浪漫派至現代室內樂與交響巨作，以嚴謹的曲式結構、豐富的配器層次與極致動態對比著稱。無論是獨奏鋼琴與弦樂四重奏的細膩呼吸，或是百人管弦樂團在音樂廳中綻放的龐大音場與自然殘響，皆是檢驗音響系統解析力、定位感與微動態重現能力的終極試金石。',
  'Jazz': '爵士樂誕生於即興創作、搖擺律動與藍調音階的交會之中，從紐奧良傳統爵士、搖擺大樂團、咆勃與酷派爵士，一路演化至融合爵士與巴薩諾瓦。薩克斯風、小號、鋼琴與低音提琴之間的即時對話，講究空間感、演奏者的細微觸鍵與現場互動火花，帶來自由而迷人的聆聽體驗。',
  'R&B': '節奏藍調與靈魂樂以飽滿的低頻律動、富有感染力的轉音唱腔與深沉的情感張力為核心。從摩城黃金年代與經典靈魂樂，到當代新靈魂與放克律動，音樂中流露著人聲的溫潤質感與絲滑節奏，讓每一段旋律都充滿律動感與親密氛圍。',
  'Stage & Screen': '舞台與影視配樂集結了電影原聲帶、百老匯音樂劇、動畫配樂與遊戲交響樂。作曲家運用主題動機、管弦編制與當代電子聲景，將畫面情緒與敘事張力化為純粹的聽覺體驗，兼具宏大戲劇張力與令人難忘的旋律記憶。',
  'International': '國際與世界音樂匯聚了來自全球各地的傳統樂器、地方音階與民族唱腔，從日本演歌、凱爾特民謠、法國香頌、葡萄牙法朵，到中式古箏、二胡、琵琶與非洲鼓樂，展現不同文化土壤孕育出的獨特音色與生命力。',
  'Vocal': '發燒人聲與聲樂作品將焦點完全凝聚於歌者的嗓音表情、氣息細節與咬字質感。搭配精簡而通透的伴奏編制，無論是經典爵士標準曲、無伴奏合唱或高解析發燒錄音，都能展現如臨現場般的口型結像與溫潤中頻密度。',
  'Country': '鄉村音樂源自美國南方與阿帕拉契山脈的民間敘事傳統，以木吉他、班卓琴、踏板電吉他與小提琴交織出質樸溫暖的聲響。真摯直白的生活故事與親切流暢的旋律線，讓鄉村音樂始終保有歷久彌新的情感共鳴。',
  'Folk': '民謠與創作歌手作品強調原聲樂器的自然質地與詩意詞作，從傳統歌謠到當代獨立民謠，以最純粹的吉他指彈、口琴與誠摯嗓音直擊人心，呈現克制而深沉的敘事魅力。',
  'Latin': '拉丁音樂融合了加勒比海、中南美洲與伊比利半島的多元節奏基因，涵蓋騷莎、探戈、佛朗明哥、森巴與波麗露。熱情的切分節奏、尼龍弦吉他與豐富打擊樂器，勾勒出鮮明立體的色彩與律動能量。',
  'Blues': '藍調是現代流行、搖滾與爵士樂的共同根源，以十二小節和聲進行、滑棒吉他與充滿滄桑張力的唱腔，傾訴生命中的憂鬱、韌性與渴望。無論是原聲三角洲藍調或芝加哥電吉他藍調，都蘊含直達靈魂的真實情感。',
  'New Age': '新世紀與氛圍音樂以寬廣空靈的音場、柔和合成器鋪陳與自然原聲樂器交織出寧靜冥想的聽覺空間，淡化強烈節拍束縛，適合深度放鬆、專注思考或深夜靜心聆聽。',
  'Easy Listening': '輕音樂與發燒示範作品以舒緩流暢的器樂演奏、精緻錄音工藝與寬鬆自然的音色平衡見長，涵蓋經典沙龍音樂、抒情器樂改編與各大發燒廠牌的試音名盤，耐聽而不具壓迫感。',
  'Holiday': '節日音樂收錄了聖誕頌歌、冬季爵士、新年維也納音樂會與各類節慶經典改編，以溫暖歡愉的管弦、合唱與原聲編制，為重要節日營造充滿儀式感與團聚氛圍的配樂。',
  'Electronic': '電子音樂以合成器、取樣器與數位訊號塑形打造無窮的聲響可能，涵蓋浩室、鐵克諾、迷幻電音、緩拍、合成波與舞曲。精準的低頻脈衝與立體聲場設計，展現前衛而富有未來感的聽覺美學。',
  "Children's": '兒童音樂包含經典兒歌、童謠、搖籃曲與寓教於樂的親子故事配樂，以明亮純真的旋律與溫柔編曲陪伴成長時光。',
  'Rap': '饒舌與嘻哈音樂以節奏鮮明的取樣節拍、紮實低頻與充滿韻腳張力的說唱敘事為核心，從黃金年代 Boom Bap 到當代多元流派，展現強烈的街頭文化脈動與語言節奏藝術。',
  'Avant-Garde': '前衛與實驗音樂勇於打破傳統調性、節奏與樂器框架，涵蓋極簡主義、自由爵士、具象音樂與跨界聲響藝術，為追求新鮮感與深度探索的樂迷開啟截然不同的聆聽維度。',
  'Religious': '宗教與聖樂作品橫跨葛利果聖歌、巴洛克彌撒曲、福音合唱、頌歌與東方禪樂佛曲，以莊嚴純淨的和聲與空間殘響，帶來撫慰心靈的沉靜力量。',
  'Reggae': '雷鬼音樂誕生於牙買加，以標誌性的反拍吉他切音、深沉彈性的貝斯線與從容搖曳的律動聞名，延伸出 Dub、Ska 等風格，傳遞陽光、自由與和平的音樂精神。',
  'Comedy/Spoken': '喜劇與口說藝術涵蓋脫口秀、相聲、詩詞朗誦與有聲書作品，聚焦於語調節奏、現場互動與文字表現力，帶來音樂之外的純粹語言魅力。'
};

const ROON_GENRE_QUERY_TERMS = {
  'Pop/Rock': 'Pop Rock',
  'Classical': 'Classical Orchestra Symphony',
  'Jazz': 'Jazz Blue Note',
  'R&B': 'R&B Soul',
  'Stage & Screen': 'Soundtrack Film Score',
  'International': 'World Music International',
  'Vocal': 'Vocal Jazz Audiophile',
  'Country': 'Country Music',
  'Folk': 'Folk Acoustic Singer Songwriter',
  'Latin': 'Latin Jazz Tango',
  'Blues': 'Blues Guitar',
  'New Age': 'New Age Ambient',
  'Easy Listening': 'Easy Listening Instrumental',
  'Holiday': 'Christmas Holiday',
  'Electronic': 'Electronic Ambient',
  "Children's": 'Children Music Lullaby',
  'Rap': 'Hip Hop Rap',
  'Avant-Garde': 'Avant-Garde Experimental',
  'Religious': 'Gospel Sacred Choral',
  'Reggae': 'Reggae Dub',
  'Comedy/Spoken': 'Spoken Word Comedy'
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

function getRoonGenreDescription(genreName) {
  const canon = canonicalizeGenre(genreName);
  return ROON_GENRE_DESCRIPTIONS[canon] || ROON_GENRE_DESCRIPTIONS['Pop/Rock'];
}

function getRoonGenreQueryTerm(genreName) {
  const canon = canonicalizeGenre(genreName);
  return ROON_GENRE_QUERY_TERMS[canon] || canon || 'Music';
}

// Renderer (classic script tag) exposure
if (typeof window !== 'undefined') {
  window.CANONICAL_GENRES = CANONICAL_GENRES;
  window.ROON_GENRE_ZH_MAP = ROON_GENRE_ZH_MAP;
  window.ROON_GENRE_DESCRIPTIONS = ROON_GENRE_DESCRIPTIONS;
  window.canonicalizeGenre = canonicalizeGenre;
  window.getRoonGenreZhName = getRoonGenreZhName;
  window.getRoonGenreDescription = getRoonGenreDescription;
  window.getRoonGenreQueryTerm = getRoonGenreQueryTerm;
}

// Node.js (main process / CLI scripts) exposure
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { CANONICAL_GENRES, ROON_GENRE_ZH_MAP, ROON_GENRE_DESCRIPTIONS, canonicalizeGenre, getRoonGenreZhName, getRoonGenreDescription, getRoonGenreQueryTerm };
}
