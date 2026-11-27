// Shared Roon-aligned 21 Music Genres classification engine.
// Mirrors and supersedes frontend/genre-utils.js rules in native Rust.

pub const CANONICAL_GENRES: [&str; 21] = [
    "Pop/Rock",
    "Classical",
    "Jazz",
    "R&B",
    "Stage & Screen",
    "International",
    "Vocal",
    "Country",
    "Folk",
    "Latin",
    "Blues",
    "New Age",
    "Easy Listening",
    "Holiday",
    "Electronic",
    "Children's",
    "Rap",
    "Avant-Garde",
    "Religious",
    "Reggae",
    "Comedy/Spoken",
];

#[inline]
fn is_ascii_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn contains_ascii_word(haystack_lower: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return false;
    }
    let hb = haystack_lower.as_bytes();
    let nb = needle_lower.as_bytes();
    let nlen = nb.len();
    let mut start = 0usize;
    while let Some(pos) = haystack_lower[start..].find(needle_lower) {
        let abs = start + pos;
        let before_ok = abs == 0 || !is_ascii_word_char(hb[abs - 1]);
        let after_idx = abs + nlen;
        let after_ok = after_idx >= hb.len() || !is_ascii_word_char(hb[after_idx]);
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
        while start < hb.len() && !haystack_lower.is_char_boundary(start) {
            start += 1;
        }
    }
    false
}

fn any_ascii_word(hay: &str, words: &[&str]) -> bool {
    words.iter().any(|w| contains_ascii_word(hay, w))
}

fn any_substr(hay: &str, subs: &[&str]) -> bool {
    subs.iter().any(|s| hay.contains(s))
}

pub fn match_roon_genre_rules(text: &str, strict_meta: bool) -> Option<&'static str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_lowercase();

    // 1. Holiday / Christmas
    if any_ascii_word(&lower, &["christmas", "holiday", "xmas", "carol", "carols"])
        || any_substr(&lower, &["聖誕", "耶誕", "節日音樂", "新年音樂會"])
    {
        return Some("Holiday");
    }

    // 2. Children's
    if any_ascii_word(&lower, &["children's", "childrens", "children", "lullaby", "lullabies", "nursery"])
        || any_substr(&lower, &["kids music", "kid music", "兒童音樂", "兒歌", "童謠"])
    {
        return Some("Children's");
    }

    // 3. Comedy / Spoken Word
    if any_ascii_word(&lower, &["comedy", "audiobook", "standup", "stand-up"])
        || any_substr(&lower, &["spoken word", "喜劇", "口說", "相聲", "朗誦", "有聲書"])
    {
        return Some("Comedy/Spoken");
    }

    // 4. Religious / Sacred / Gospel
    if any_ascii_word(&lower, &["religious", "gospel", "sacred", "christian", "worship", "hymn", "hymns", "chant", "gregorian"])
        || any_substr(&lower, &["宗教", "聖歌", "福音", "佛樂", "禪樂"])
    {
        return Some("Religious");
    }

    // 5. Stage & Screen
    if any_ascii_word(&lower, &["soundtrack", "ost", "o.s.t", "cinematic", "musical", "broadway", "anime", "vocaloid"])
        || any_substr(&lower, &[
            "stage & screen", "stage and screen", "film score", "movie score", "game music",
            "bandes originales", "tvアニメ", "アニメ", "サウンドトラック", "イメージ・ソング",
            "オープニングテーマ", "エンディングテーマ", "ゲーム", "動畫音樂", "動畫影視音樂",
            "原聲帶", "原聲", "配樂", "影視", "動漫", "舞台", "音樂劇", "highlights from",
            "sunset boulevard", "jekyll & hyde", "jekyll and hyde", "zombie land saga",
            "ゾンビランドサガ", "franchouchou", "death musume", "myth & roid", "君の名は",
            "天気の子", "となりのトトロ", "仙劍奇俠傳", "イース", "symphony ys", "final fantasy",
            "studio ghibli", "久石譲", "久石讓", "澤野弘之", "梶浦由記", "菅野よう子",
            "hans zimmer", "john williams", "ennio morricone",
        ])
    {
        return Some("Stage & Screen");
    }

    // 6. Classical
    if any_ascii_word(&lower, &[
        "classical", "orchestra", "orchestral", "symphony", "symphonic", "baroque", "opera",
        "philharmonic", "concerto", "sonata", "requiem", "nocturne", "mozart", "beethoven",
        "bach", "chopin", "tchaikovsky", "brahms", "handel", "vivaldi", "haydn", "schubert",
        "schumann", "mahler", "debussy", "ravel", "rachmaninoff", "prokofiev", "shostakovich", "orff",
    ]) || any_substr(&lower, &[
        "chamber music", "string quartet", "cello suite", "carmina burana",
        "古典", "交響曲", "交響樂", "協奏曲", "奏鳴曲", "管弦樂", "室內樂", "歌劇",
        "弦樂四重奏", "愛樂管弦", "明尼蘇達管弦",
    ]) {
        return Some("Classical");
    }

    // 7. Jazz
    if any_ascii_word(&lower, &["jazz", "swing", "bebop", "dixieland"])
        || any_substr(&lower, &[
            "jazz fusion", "hard bop", "big band", "smooth jazz", "bossa nova", "blue bossa",
            "sheffield jazz", "miles davis", "john coltrane", "bill evans", "chet baker",
            "diana krall", "keith jarrett", "oscar peterson", "dave brubeck", "herbie hancock",
            "stan getz", "ella fitzgerald", "louis armstrong", "pat metheny", "norah jones",
            "jacintha", "stacey kent", "anne bisson", "爵士", "巴薩諾瓦",
        ])
    {
        return Some("Jazz");
    }

    // 8. Latin
    if any_ascii_word(&lower, &["latin", "salsa", "tango", "flamenco", "samba", "mambo", "bolero", "bachata", "reggaeton"])
        || any_substr(&lower, &["astor piazzolla", "buena vista social club", "拉丁", "探戈", "佛朗明哥"])
    {
        return Some("Latin");
    }

    // 9. Reggae
    if any_ascii_word(&lower, &["reggae", "dub", "ska", "dancehall"])
        || any_substr(&lower, &["bob marley", "雷鬼"])
    {
        return Some("Reggae");
    }

    // 10. Blues
    if any_ascii_word(&lower, &["blues"])
        || any_substr(&lower, &[
            "delta blues", "chicago blues", "electric blues", "b.b. king", "b. b. king",
            "bb king", "stevie ray vaughan", "muddy waters", "robert johnson", "藍調",
        ])
    {
        return Some("Blues");
    }

    // 11. R&B / Soul / Funk
    if any_ascii_word(&lower, &["r&b", "rnb", "motown", "neo-soul", "neosoul", "funk"])
        || any_substr(&lower, &[
            "rhythm and blues", "marvin gaye", "stevie wonder", "aretha franklin",
            "alicia keys", "amy winehouse", "lisa stansfield", "節奏藍調", "靈魂樂",
        ])
        || (!strict_meta && contains_ascii_word(&lower, "soul"))
    {
        return Some("R&B");
    }

    // 12. Rap / Hip-Hop
    if any_ascii_word(&lower, &["hip-hop", "hiphop", "rap"])
        || any_substr(&lower, &[
            "hip hop", "trap music", "boom bap", "kendrick lamar", "kanye west",
            "eminem", "jay-z", "jay z", "notorious b.i.g", "tupac", "嘻哈", "饒舌",
        ])
    {
        return Some("Rap");
    }

    // 13. Country
    if any_ascii_word(&lower, &["country", "bluegrass", "nashville", "americana"])
        || any_substr(&lower, &["dolly parton", "johnny cash", "willie nelson", "garth brooks", "鄉村"])
    {
        return Some("Country");
    }

    // 14. Folk
    if any_ascii_word(&lower, &["folk", "singer-songwriter", "singer/songwriter"])
        || any_substr(&lower, &[
            "joan baez", "joan c. baez", "bob dylan", "simon & garfunkel", "joni mitchell",
            "nick drake", "james taylor", "leonard cohen", "indigo girls",
            "richard & linda thompson", "许巍", "許巍", "民謠", "民歌",
        ])
    {
        return Some("Folk");
    }

    // 15. International / World / Ethnic / Traditional
    if any_ascii_word(&lower, &["international", "celtic", "afrobeat", "chanson", "fado", "gamelan", "taiko", "enka", "ondekoza", "hélène", "helene"])
        || any_substr(&lower, &[
            "world music", "演歌", "鬼太鼓座", "馬頭琴", "民樂", "國樂", "古箏", "二胡",
            "琵琶", "笛子", "國際音樂", "世界音樂", "民族音樂", "阿姐鼓", "朱哲琴",
        ])
        || (!strict_meta && contains_ascii_word(&lower, "world"))
    {
        return Some("International");
    }

    // 16. Avant-Garde / Experimental
    if any_ascii_word(&lower, &["avant-garde", "avantgarde", "experimental", "noise", "minimalism"])
        || any_substr(&lower, &["free jazz", "musique concr", "前衛", "實驗音樂"])
    {
        return Some("Avant-Garde");
    }

    // 17. New Age / Ambient
    if any_ascii_word(&lower, &["ambient", "meditation", "drone", "atmospheric", "enya", "vangelis", "yanni"])
        || any_substr(&lower, &["new age", "contemporary instrumental", "喜多郎", "坂本龍一", "新世紀", "氛圍", "冥想"])
    {
        return Some("New Age");
    }

    // 18. Electronic
    if any_ascii_word(&lower, &[
        "electronic", "electronica", "techno", "trance", "synthwave", "synth-pop", "synthpop",
        "edm", "downtempo", "chillout", "dubstep", "disco", "kraftwerk",
    ]) || any_substr(&lower, &[
        "deep house", "house music", "drum & bass", "drum and bass", "daft punk",
        "aphex twin", "massive attack", "portishead", "chemical brothers", "電子", "電音",
    ]) || (!strict_meta && contains_ascii_word(&lower, "dance"))
    {
        return Some("Electronic");
    }

    // 19. Vocal
    if any_ascii_word(&lower, &["acapella", "choral"])
        || any_substr(&lower, &[
            "vocal jazz", "audiophile vocal", "a cappella", "susan wong", "amanda mcbroom",
            "eva cassidy", "emi fujita", "藤田惠美", "蔡琴", "陳潔麗", "趙鵬", "童麗", "王聞",
            "玉置浩二", "tony bennett", "frank sinatra", "發燒人聲", "人聲", "美聲", "聲樂",
        ])
        || (!strict_meta && contains_ascii_word(&lower, "vocal"))
    {
        return Some("Vocal");
    }

    // 20. Easy Listening
    if any_ascii_word(&lower, &["lounge", "exotica", "muzak", "audiophile", "binaural", "burmester", "stockfisch"])
        || any_substr(&lower, &[
            "easy listening", "instrumental pop", "bowers & wilkins", "chesky records", "chesky record",
            "sheffield lab", "reference recordings", "opus 3", "telarc sampler", "hi-end",
            "音響大展", "音響展", "展cd", "示範片", "測試片", "終極測試", "發燒試音",
            "試音碟", "發燒天碟", "輕音樂", "抒情演奏",
        ])
        || (!strict_meta && contains_ascii_word(&lower, "instrumental"))
    {
        return Some("Easy Listening");
    }

    // 21. Pop/Rock
    if any_ascii_word(&lower, &[
        "pop", "rock", "metal", "punk", "grunge", "alternative", "indie", "psychedelic",
        "britpop", "j-rock", "jrock", "j-pop", "jpop", "c-pop", "cpop", "k-pop", "kpop",
        "hk-pop", "cantopop", "mandopop", "kayokyoku", "ballad", "coldplay", "queen",
        "radiohead", "nirvana", "eagles", "oasis", "muse", "radwimps", "aimer", "aiko",
        "daoko", "reol", "majiko", "garnidelia", "egoist", "junna", "adele",
    ]) || any_substr(&lower, &[
        "hard rock", "classic rock", "prog rock", "progressive rock", "asian music",
        "pink floyd", "led zeppelin", "the beatles", "rolling stones", "dire straits",
        "fleetwood mac", "deep purple", "ac/dc", "linkin park", "tears for fears",
        "king gnu", "back number", "one ok rock", "carole king", "まじ娘", "浜崎あゆみ",
        "熊木杏里", "西村知美", "周杰倫", "張學友", "王菲", "陳奕迅", "林俊傑", "鄧麗君",
        "黃舒駿", "shu jun huang", "飛兒樂團", "f.i.r", "自然捲", "五月天", "伍佰",
        "老歌", "流行", "搖滾", "重金屬", "華語", "一般流行",
    ]) {
        return Some("Pop/Rock");
    }

    None
}

fn find_canonical_exact(lower: &str) -> Option<&'static str> {
    CANONICAL_GENRES
        .iter()
        .copied()
        .find(|c| c.eq_ignore_ascii_case(lower))
}

fn map_legacy_genre(lower: &str) -> Option<&'static str> {
    match lower {
        "pop" | "rock" | "metal" => Some("Pop/Rock"),
        "soundtrack" => Some("Stage & Screen"),
        "soul / r&b" => Some("R&B"),
        "ambient" => Some("New Age"),
        "hip hop" => Some("Rap"),
        "audiophile" => Some("Easy Listening"),
        _ => None,
    }
}

fn is_generic_or_legacy(lower: &str) -> bool {
    matches!(
        lower,
        "" | "unknown" | "various" | "other" | "music" | "none" | "test"
    ) || find_canonical_exact(lower).is_some()
        || map_legacy_genre(lower).is_some()
}

pub fn canonicalize_genre(
    raw_genre: &str,
    track_raw_genre: &str,
    online_genre: &str,
    title: &str,
    album: &str,
    artist: &str,
    album_artist: &str,
) -> &'static str {
    let g = raw_genre.trim();
    let track_raw = track_raw_genre.trim();
    let online_raw = online_genre.trim();
    let g_lower = g.to_lowercase();
    let track_raw_lower = track_raw.to_lowercase();

    let explicit_raw = if !is_generic_or_legacy(&track_raw_lower) {
        track_raw
    } else if !is_generic_or_legacy(&g_lower) {
        g
    } else {
        ""
    };

    // Priority 1: Explicit non-canonical file genre tag
    if !explicit_raw.is_empty() {
        if let Some(m) = match_roon_genre_rules(explicit_raw, false) {
            return m;
        }
    }

    // Priority 1b: Already-valid specific canonical genre (non-Pop/Rock default)
    if !g.is_empty() && g_lower != "pop/rock" {
        if let Some(exact) = find_canonical_exact(&g_lower) {
            return exact;
        }
    }

    // Priority 2: Online structured genre tag
    if !online_raw.is_empty() {
        let ol = online_raw.to_lowercase();
        if let Some(exact) = find_canonical_exact(&ol) {
            return exact;
        }
        if let Some(m) = match_roon_genre_rules(online_raw, false) {
            return m;
        }
    }

    // Priority 3: Strict metadata inference from album/title + artist (never file path)
    let title_part = if album.trim().is_empty() { title } else { album };
    let meta_text = format!("{} {} {}", title_part, album_artist, artist);
    if let Some(m) = match_roon_genre_rules(&meta_text, true) {
        return m;
    }

    // Priority 4: Existing canonical genre (including Pop/Rock)
    if !g.is_empty() {
        if let Some(exact) = find_canonical_exact(&g_lower) {
            return exact;
        }
    }

    // Priority 5: Legacy bucket
    if let Some(leg) = map_legacy_genre(&g_lower) {
        return leg;
    }

    // Priority 6: Loose fallback
    if !g.is_empty() {
        if let Some(loose) = match_roon_genre_rules(g, false) {
            return loose;
        }
    }

    "Pop/Rock"
}

pub fn get_roon_genre_zh_name(genre: &str) -> &'static str {
    let canon = if let Some(exact) = find_canonical_exact(&genre.trim().to_lowercase()) {
        exact
    } else {
        canonicalize_genre(genre, "", "", "", "", "", "")
    };
    match canon {
        "Pop/Rock" => "流行 / 搖滾",
        "Classical" => "古典樂",
        "Jazz" => "爵士樂",
        "R&B" => "節奏藍調",
        "Stage & Screen" => "舞台與影視",
        "International" => "國際音樂",
        "Vocal" => "人聲",
        "Country" => "鄉村音樂",
        "Folk" => "民謠",
        "Latin" => "拉丁音樂",
        "Blues" => "藍調",
        "New Age" => "新世紀音樂",
        "Easy Listening" => "輕音樂",
        "Holiday" => "節日音樂",
        "Electronic" => "電子音樂",
        "Children's" => "兒童音樂",
        "Rap" => "饒舌",
        "Avant-Garde" => "前衛音樂",
        "Religious" => "宗教",
        "Reggae" => "雷鬼",
        "Comedy/Spoken" => "喜劇 / 口說藝術",
        _ => "流行 / 搖滾",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_genre_count_and_zh_names() {
        assert_eq!(CANONICAL_GENRES.len(), 21);
        for g in CANONICAL_GENRES {
            let zh = get_roon_genre_zh_name(g);
            assert!(!zh.is_empty());
        }
    }

    #[test]
    fn test_canonicalize_priority_and_word_boundaries() {
        // Explicit file genre tag overrides Pop/Rock or generic defaults
        assert_eq!(
            canonicalize_genre("Unknown", "Jazz Fusion", "", "Some Track", "Some Album", "Some Artist", ""),
            "Jazz"
        );
        // Online structured genre overrides generic Pop/Rock when file tag is empty
        assert_eq!(
            canonicalize_genre("Pop/Rock", "", "Bossa Nova", "Track", "Album", "Artist", ""),
            "Jazz"
        );
        // Strict metadata inference from artist/album
        assert_eq!(
            canonicalize_genre("Unknown", "", "", "Kind of Blue", "Kind of Blue", "Miles Davis", ""),
            "Jazz"
        );
        assert_eq!(
            canonicalize_genre("Unknown", "", "", "Nocturne", "Nocturnes", "Chopin", ""),
            "Classical"
        );
        assert_eq!(
            canonicalize_genre("Unknown", "", "", "七里香", "七里香", "周杰倫", ""),
            "Pop/Rock"
        );
        // Word boundary: "Soul" must not falsely match inside "Seoul"
        assert_ne!(
            canonicalize_genre("Unknown", "", "", "Seoul Night", "Seoul Night", "Unknown", ""),
            "R&B"
        );
    }
}
