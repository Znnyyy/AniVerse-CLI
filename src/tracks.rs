use std::collections::HashMap;

use colored::Colorize;

/// Satu track (audio/subtitle) yang dideklarasikan di master playlist HLS.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaylistTrack {
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaylistTracks {
    pub audios: Vec<PlaylistTrack>,
    pub subtitles: Vec<PlaylistTrack>,
}

/// Pilihan track user (None = biarkan otomatis).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackChoice {
    pub sub_url: Option<String>,
    /// Indeks audio 1-based (sesuai penomoran `aid` mpv/ffmpeg pada HLS).
    pub audio_index: Option<u32>,
}

/// Resolusi URI relatif di master playlist terhadap URL playlist induk.
pub fn resolve_playlist_url(base: &str, uri: &str) -> String {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return uri.to_string();
    }
    if let Some(idx) = base.find("://") {
        let host_start = idx + 3;
        let rest = &base[host_start..];
        if let Some(slash) = rest.find('/') {
            let origin = &base[..host_start + slash];
            if let Some(stripped) = uri.strip_prefix('/') {
                return format!("{}/{}", origin, stripped);
            }
            let dir_end = base.rfind('/').map(|i| i + 1).unwrap_or(base.len());
            return format!("{}{}", &base[..dir_end], uri);
        }
    }
    uri.to_string()
}

/// Parse atribut `#EXT-X-MEDIA:KEY="nilai",KEY2=nilai2` menjadi map.
pub fn parse_attributes(line: &str) -> HashMap<String, String> {
    let attrs = match line.find(':') {
        Some(i) => &line[i + 1..],
        None => line,
    };
    let mut map = HashMap::new();
    let mut rest = attrs;
    while !rest.is_empty() {
        rest = rest.trim_start_matches(',').trim_start();
        let Some(eq) = rest.find('=') else {
            break;
        };
        let key = rest[..eq].trim().to_string();
        let mut tail = &rest[eq + 1..];
        let value = match tail.strip_prefix('"') {
            Some(stripped) => match stripped.find('"') {
                Some(end) => {
                    let v = stripped[..end].to_string();
                    tail = &stripped[end + 1..];
                    v
                }
                None => {
                    let v = stripped.to_string();
                    tail = "";
                    v
                }
            },
            None => {
                let end = tail.find(',').unwrap_or(tail.len());
                let v = tail[..end].to_string();
                tail = &tail[end..];
                v
            }
        };
        map.insert(key, value);
        rest = tail;
    }
    map
}

/// Parse master playlist HLS: kumpulkan entry `#EXT-X-MEDIA` tipe AUDIO & SUBTITLES.
pub fn parse_master_playlist(text: &str, base_url: &str) -> PlaylistTracks {
    let mut tracks = PlaylistTracks::default();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("#EXT-X-MEDIA:") {
            continue;
        }
        let attrs = parse_attributes(trimmed);
        let name = attrs
            .get("NAME")
            .cloned()
            .unwrap_or_else(|| "Tanpa nama".to_string());
        let url = attrs.get("URI").map(|u| resolve_playlist_url(base_url, u));
        match attrs.get("TYPE").map(String::as_str) {
            Some("AUDIO") => tracks.audios.push(PlaylistTrack { name, url }),
            Some("SUBTITLES") => tracks.subtitles.push(PlaylistTrack { name, url }),
            _ => {}
        }
    }
    tracks
}

/// Ambil Referer yang sesuai provider untuk request playlist.
fn referer_for(provider_type: usize) -> Option<&'static str> {
    match provider_type {
        crate::PROVIDER_OTAKUDESU => Some("https://desustream.net/"),
        crate::PROVIDER_IDLIX => Some("https://z2.idlixku.com/"),
        crate::PROVIDER_SAMEHADAKU => Some("https://samehadaku.li/"),
        crate::PROVIDER_KUSONIME => Some("https://kusonime.com/"),
        _ => None,
    }
}

/// Fetch master playlist (kalau URL m3u8) dan tampilkan menu pemilihan track.
/// Mengembalikan `None` kalau stream bukan HLS atau tidak ada track tambahan.
pub async fn probe_and_choose(stream_url: &str, provider_type: usize) -> Option<TrackChoice> {
    if !stream_url.contains("m3u8") {
        return None;
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;
    let mut req = client.get(stream_url);
    if let Some(referer) = referer_for(provider_type) {
        req = req.header("Referer", referer);
    }
    let text = req.send().await.ok()?.text().await.ok()?;
    if !text.contains("#EXT-X-MEDIA") {
        return None;
    }
    let tracks = parse_master_playlist(&text, stream_url);
    if tracks.audios.is_empty() && tracks.subtitles.is_empty() {
        return None;
    }

    let mut sub_url: Option<String> = None;
    if !tracks.subtitles.is_empty() {
        let mut items = vec!["Subtitle: Otomatis (bawaan stream)".to_string()];
        for t in &tracks.subtitles {
            items.push(t.name.clone());
        }
        let sel = dialoguer::Select::with_theme(&crate::util::custom_theme())
            .with_prompt("Pilih Subtitle: (Esc untuk otomatis)")
            .default(0)
            .items(&items)
            .interact_opt()
            .ok()
            .flatten();
        if let Some(i) = sel {
            if i > 0 {
                sub_url = tracks.subtitles[i - 1].url.clone();
            }
        }
    }

    let mut audio_index: Option<u32> = None;
    if !tracks.audios.is_empty() {
        let mut items = vec!["Audio: Otomatis".to_string()];
        for t in &tracks.audios {
            items.push(t.name.clone());
        }
        let sel = dialoguer::Select::with_theme(&crate::util::custom_theme())
            .with_prompt("Pilih Audio: (Esc untuk otomatis)")
            .default(0)
            .items(&items)
            .interact_opt()
            .ok()
            .flatten();
        if let Some(i) = sel {
            if i > 0 {
                audio_index = Some(i as u32);
            }
        }
    }

    if sub_url.is_none() && audio_index.is_none() {
        println!("  {} Track tambahan: pakai bawaan stream.", "ℹ".yellow());
    }
    Some(TrackChoice {
        sub_url,
        audio_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_playlist_url_variants() {
        let base = "https://cdn.example.com/hls/master.m3u8?token=abc";
        assert_eq!(
            resolve_playlist_url(base, "https://other.example.com/x.m3u8"),
            "https://other.example.com/x.m3u8"
        );
        assert_eq!(
            resolve_playlist_url(base, "/abs/path.m3u8"),
            "https://cdn.example.com/abs/path.m3u8"
        );
        assert_eq!(
            resolve_playlist_url(base, "subs/id.m3u8"),
            "https://cdn.example.com/hls/subs/id.m3u8"
        );
    }

    #[test]
    fn parse_attributes_quotes_and_plain() {
        let line = r#"#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="Indonesia, Softsub",DEFAULT=YES,URI="subs/id.m3u8""#;
        let attrs = parse_attributes(line);
        assert_eq!(attrs.get("TYPE").unwrap(), "SUBTITLES");
        assert_eq!(attrs.get("NAME").unwrap(), "Indonesia, Softsub");
        assert_eq!(attrs.get("URI").unwrap(), "subs/id.m3u8");
        assert_eq!(attrs.get("DEFAULT").unwrap(), "YES");
    }

    #[test]
    fn parse_master_playlist_collects_audio_and_subs() {
        let master = r#"#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="Japanese",DEFAULT=YES,URI="audio/jp.m3u8"
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="Indonesian Dub",URI="audio/id.m3u8"
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="Indonesia",URI="sub/id.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=1000000,RESOLUTION=1280x720,AUDIO="aud",SUBTITLES="subs"
720p.m3u8
"#;
        let base = "https://cdn.example.com/hls/master.m3u8";
        let tracks = parse_master_playlist(master, base);
        assert_eq!(tracks.audios.len(), 2);
        assert_eq!(tracks.audios[0].name, "Japanese");
        assert_eq!(
            tracks.audios[0].url.as_deref(),
            Some("https://cdn.example.com/hls/audio/jp.m3u8")
        );
        assert_eq!(tracks.subtitles.len(), 1);
        assert_eq!(
            tracks.subtitles[0].url.as_deref(),
            Some("https://cdn.example.com/hls/sub/id.m3u8")
        );
    }

    #[test]
    fn parse_master_playlist_no_media_returns_empty() {
        let media_only = "#EXTM3U\n#EXTINF:10.0,\nseg1.ts\n#EXT-X-ENDLIST\n";
        let tracks = parse_master_playlist(media_only, "https://cdn.example.com/v/index.m3u8");
        assert!(tracks.audios.is_empty() && tracks.subtitles.is_empty());
    }
}
