use base64::prelude::*;
use std::collections::HashMap;
use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;

use crate::{
    ext::Ext,
    models::{Episode, Meta, Movie, Stream},
};

pub struct Otakudesu {
    client: Client,
    base_url: String,
}

impl Otakudesu {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/110.0.0.0 Safari/537.36")
                .build()
                .unwrap(),
            base_url: "https://otakudesu.blog".to_string(),
        }
    }

    /// Ambil URL video langsung dari halaman embed mana pun (desustream, vidhide, dll).
    /// Mengembalikan `None` jika halaman embed ternyata maintenance / bukan video,
    /// agar URL halaman HTML tidak sampai dikirim ke pemutar.
    async fn resolve_stream_url(&self, page_url: &str) -> Option<String> {
        let page_url = clean_url(page_url);
        if page_url.contains("/maintenance") {
            return None;
        }
        if looks_like_media_url(&page_url) {
            return Some(page_url);
        }
        let Ok(resp) = self.client.get(&page_url).send().await else {
            return Some(page_url);
        };
        if resp.url().path().contains("/maintenance") {
            return None;
        }
        let origin = origin_of(resp.url());
        let Ok(text) = resp.text().await else {
            return Some(page_url);
        };
        if let Some(found) = parse_embed_url(&text) {
            return Some(found);
        }
        if text.contains("eval(function(p,a,c,k,e,d)") {
            if let Some(unpacked) = unpack_packed_js(&text) {
                if let Some(found) = parse_vidhide_url(&unpacked, &origin) {
                    return Some(found);
                }
                if let Some(found) = parse_embed_url(&unpacked) {
                    return Some(found);
                }
            }
        }
        if text.to_lowercase().contains("maintenance") {
            return None;
        }
        Some(page_url)
    }
}

/// Bersihkan URL hasil ekstraksi HTML mentah: decode entitas (`&amp;` → `&`),
/// hapus escape JSON (`\/` → `/`), dan trim spasi.
fn clean_url(raw: &str) -> String {
    let decoded = html_escape::decode_html_entities(raw).into_owned();
    decoded.replace("\\/", "/").trim().to_string()
}

/// Ekstrak URL video langsung dari isi halaman embed (HTML/JS mentah).
fn parse_embed_url(text: &str) -> Option<String> {
    if let Some(src_start) = text.find("<source src=\"") {
        let rest = &text[src_start + "<source src=\"".len()..];
        if let Some(end) = rest.find('"') {
            return Some(clean_url(&rest[..end]));
        }
    }
    if let Some(file_start) = text.find("otakudesu('{\"file\":\"") {
        let rest = &text[file_start + "otakudesu('{\"file\":\"".len()..];
        if let Some(end) = rest.find('"') {
            return Some(clean_url(&rest[..end]));
        }
    }
    if let Some(file_start) = text.find("\"file\":\"") {
        let rest = &text[file_start + "\"file\":\"".len()..];
        if let Some(end) = rest.find('"') {
            return Some(clean_url(&rest[..end]));
        }
    }
    None
}

/// URL yang sudah pasti file media sehingga bisa langsung diputar tanpa fetch halaman.
fn looks_like_media_url(url: &str) -> bool {
    let u = url.to_lowercase();
    u.contains(".m3u8") || u.contains(".mp4") || u.contains(".mpd") || u.contains(".webm") || u.contains("videoplayback")
}

/// Ambil origin (`scheme://host`) dari URL untuk merelatifkan path embed.
fn origin_of(url: &reqwest::Url) -> String {
    match url.host_str() {
        Some(host) => format!("{}://{}", url.scheme(), host),
        None => String::new(),
    }
}

/// Buka JavaScript yang di-pack gaya Dean Edwards:
/// `eval(function(p,a,c,k,e,d){...}('payload',base,count,'dict'.split('|')))`.
/// Token di payload adalah angka berbasis `base` yang dipetakan ke `dict`,
/// persis seperti unpacker aslinya di browser.
fn unpack_packed_js(text: &str) -> Option<String> {
    let re = Regex::new(
        r"eval\(function\(p,a,c,k,e,d\)\{[^}]*\}\('((?:[^'\\]|\\.)*)',(\d+),(\d+),'((?:[^'\\]|\\.)*)'\.split\('\|'\)\)",
    )
    .ok()?;
    let caps = re.captures(text)?;
    let payload = caps[1].replace("\\'", "'").replace("\\\"", "\"").replace("\\\\", "\\");
    let base: u32 = caps[2].parse().ok()?;
    let count: usize = caps[3].parse().ok()?;
    let dict: Vec<&str> = caps[4].split('|').collect();

    let token_re = Regex::new(r"\b[0-9a-z]+\b").ok()?;
    let mut out = String::with_capacity(payload.len() * 3);
    let mut last = 0;
    for m in token_re.find_iter(&payload) {
        out.push_str(&payload[last..m.start()]);
        let tok = m.as_str();
        match u32::from_str_radix(tok, base) {
            Ok(i) if (i as usize) < count && (i as usize) < dict.len() && !dict[i as usize].is_empty() => {
                out.push_str(dict[i as usize]);
            }
            _ => out.push_str(tok),
        }
        last = m.end();
    }
    out.push_str(&payload[last..]);
    Some(out)
}

/// Ekstrak URL stream dari JS player yang sudah di-unpack (gaya VidHide/EarnVids).
/// Ikuti preferensi player (`hls4 || hls3 || hls2`) tapi utamakan yang absolut
/// dan tidak bergantung pada referer.
fn parse_vidhide_url(unpacked: &str, origin: &str) -> Option<String> {
    for key in ["\"hls4\":\"", "\"hls2\":\"", "\"hls3\":\"", "\"file\":\""] {
        let Some(pos) = unpacked.find(key) else {
            continue;
        };
        let rest = &unpacked[pos + key.len()..];
        let Some(end) = rest.find('"') else {
            continue;
        };
        let value = clean_url(&rest[..end]);
        if value.is_empty() {
            continue;
        }
        if value.starts_with('/') {
            return Some(format!("{}{}", origin, value));
        }
        return Some(value);
    }
    None
}

#[async_trait]
impl Ext for Otakudesu {
    async fn search(&mut self, title: String, page: usize) -> anyhow::Result<(Vec<Movie>, u64)> {
        let query = urlencoding::encode(&title);
        let url = if page > 1 {
            format!("{}/page/{}/?s={}&post_type=anime", self.base_url, page, query)
        } else {
            format!("{}/?s={}&post_type=anime", self.base_url, query)
        };
        let html_text = self.client.get(&url).send().await?.text().await?;
        let document = Html::parse_document(&html_text);
        let selector = Selector::parse("ul.chivsrc li").unwrap();
        let a_selector = Selector::parse("h2 a").unwrap();
        
        let mut movies = vec![];
        for element in document.select(&selector) {
            if let Some(a_el) = element.select(&a_selector).next() {
                let text = a_el.inner_html().trim().to_string();
                let href = a_el.value().attr("href").unwrap_or("").to_string();
                movies.push(Movie {
                    id: href,
                    title: text,
                    total_episodes: None,
                });
            }
        }
        
        // Otakudesu memakai paginasi WordPress (/page/N/) sehingga total dihitung per halaman
        Ok((movies, 1))
    }

    fn supports_pagination(&self) -> bool {
        true
    }

    async fn get_episodes(&self, movie: Movie) -> anyhow::Result<(Vec<Episode>, Meta)> {
        let html_text = self.client.get(&movie.id).send().await?.text().await?;
        let document = Html::parse_document(&html_text);
        
        let ep_list_selector = Selector::parse(".episodelist ul li").unwrap();
        let a_selector = Selector::parse("span a").unwrap();
        let mut episodes = vec![];
        

        for element in document.select(&ep_list_selector) {
            if let Some(a_el) = element.select(&a_selector).next() {
                let text = a_el.inner_html().trim().to_string();
                let href = a_el.value().attr("href").unwrap_or("").to_string();
                if text.to_lowercase().contains("batch") || href.contains("/lengkap/") || href.contains("/batch/") {
                    continue;
                }
                episodes.push(Episode {
                    id: href,
                    title: text,
                    is_series: true,
                });
            }
        }
        episodes.reverse();
        
        // Meta (optional)
        let mut meta = Meta {
            thumb_url: None,
            data: vec![],
        };

        if let Ok(img_sel) = Selector::parse("img.attachment-post-thumbnail") {
            if let Some(img) = document.select(&img_sel).next() {
                if let Some(src) = img.value().attr("src") {
                    meta.thumb_url = Some(src.to_string());
                }
            }
        }
        
        if let Ok(info_sel) = Selector::parse(".infozingle p") {
            for p in document.select(&info_sel) {
                let clean = p.text().collect::<Vec<_>>().join(" ");
                if let Some(idx) = clean.find(":") {
                    meta.data.push((clean[..idx].trim().to_string(), clean[idx+1..].trim().to_string()));
                } else {
                    meta.data.push(("".to_string(), clean));
                }
            }
        }

        if let Ok(sinop_sel) = Selector::parse(".sinopc p") {
            let mut sinopsis_text = String::new();
            for p in document.select(&sinop_sel) {
                let clean = p.text().collect::<Vec<_>>().join(" ");
                let lower = clean.to_lowercase();
                if lower.contains("tonton juga") || lower.contains("kelanjutannya") || lower.contains("batch") {
                    continue;
                }
                if !sinopsis_text.is_empty() {
                    sinopsis_text.push(' ');
                }
                sinopsis_text.push_str(&clean);
            }
            if sinopsis_text.len() > 320 {
                let mut truncated = sinopsis_text.chars().take(320).collect::<String>();
                truncated.push_str("...");
                meta.data.push(("Sinopsis".to_string(), truncated));
            } else if !sinopsis_text.is_empty() {
                meta.data.push(("Sinopsis".to_string(), sinopsis_text));
            }
        }

        Ok((episodes, meta))
    }

    async fn get_stream_urls(&self, episode: Episode) -> anyhow::Result<Vec<Stream>> {
        let ep_url = episode.id.clone();
        let html_text = self.client.get(&ep_url).send().await?.text().await?;
        
        let mut streams = vec![];
        let mut mirrors = vec![];
        let mut default_url = None;

        {
            let document = Html::parse_document(&html_text);
            let iframe_sel = Selector::parse("#pembed iframe, .player-embed iframe").unwrap();
            if let Some(iframe) = document.select(&iframe_sel).next() {
                if let Some(src) = iframe.value().attr("src") {
                    let mut url = clean_url(src);
                    if url.starts_with("//") {
                        url = format!("https:{}", url);
                    }
                    default_url = Some(url);
                }
            }

            let mirror_sel = Selector::parse(".mirrorstream ul li a").unwrap();
            for a_el in document.select(&mirror_sel) {
                let text = a_el.inner_html().trim().to_string();
                let data_content = a_el.value().attr("data-content").unwrap_or("").to_string();
                mirrors.push((text, data_content));
            }
        }

        if let Some(url) = default_url {
            if let Some(url) = self.resolve_stream_url(&url).await {
                streams.push(Stream {
                    url,
                    title: "Default".to_string(),
                });
            }
        }

        let params = [("action", "aa1208d27f29ca340c92c66d1926f13f")];
        let ajax_url = format!("{}/wp-admin/admin-ajax.php", self.base_url);
        let res = self.client.post(&ajax_url)
            .header("Referer", &ep_url)
            .form(&params)
            .send().await?;
            
        let json_val: Value = res.json().await.unwrap_or(Value::Null);
        let nonce = if let Some(n) = json_val.get("data").and_then(|v| v.as_str()) {
            n.to_string()
        } else {
            return Ok(streams);
        };

        for (name, content) in mirrors {
            let decoded_bytes = match base64::prelude::BASE64_STANDARD.decode(&content) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let decoded_str = match String::from_utf8(decoded_bytes) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let payload: HashMap<String, String> = match serde_json::from_str(&decoded_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let mut form = HashMap::new();
            form.insert("action".to_string(), "2a3505c93b0035d3f455df82bf976b84".to_string());
            form.insert("nonce".to_string(), nonce.clone());
            for (k, v) in payload {
                form.insert(k, v);
            }
            
            if let Ok(m_resp) = self.client.post(&ajax_url).header("Referer", &ep_url).form(&form).send().await {
                if let Ok(m_json) = m_resp.json::<Value>().await {
                    if let Some(data) = m_json.get("data").and_then(|v| v.as_str()) {
                        if let Ok(html_bytes) = base64::prelude::BASE64_STANDARD.decode(data) {
                            if let Ok(html_str) = String::from_utf8(html_bytes) {
                                if let Some(src_start) = html_str.find("src=\"") {
                                    let rest = &html_str[src_start + 5..];
                                    if let Some(src_end) = rest.find('"') {
                                        let mut url = clean_url(&rest[..src_end]);
                                        if url.starts_with("//") {
                                            url = format!("https:{}", url);
                                        }
                                        if let Some(url) = self.resolve_stream_url(&url).await {
                                            streams.push(Stream {
                                                url,
                                                title: format!("Mirror - {}", name),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(streams)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_url_decodes_html_entities() {
        let raw = "https://rr2---sn-ojq4f5-c9.googlevideo.com/videoplayback?expire=1792017116&amp;ei=XJLPapfCLMja0uoPi8TBsQo&amp;ip=2a04:3543:1000:2310:30da:13ff:fead:6be6";
        let expected = "https://rr2---sn-ojq4f5-c9.googlevideo.com/videoplayback?expire=1792017116&ei=XJLPapfCLMja0uoPi8TBsQo&ip=2a04:3543:1000:2310:30da:13ff:fead:6be6";
        assert_eq!(clean_url(raw), expected);
        assert!(!clean_url(raw).contains("amp;"));
    }

    #[test]
    fn clean_url_decodes_json_escape_and_trims() {
        assert_eq!(
            clean_url("https:\\/\\/cdn.example.com\\/v.mp4 "),
            "https://cdn.example.com/v.mp4"
        );
    }

    #[test]
    fn parse_embed_url_from_source_tag() {
        let html = r#"<html><video><source src="https://rr.googlevideo.com/videoplayback?a=1&amp;b=2&amp;mime=video/mp4" type="video/mp4"></video></html>"#;
        assert_eq!(
            parse_embed_url(html).unwrap(),
            "https://rr.googlevideo.com/videoplayback?a=1&b=2&mime=video/mp4"
        );
    }

    #[test]
    fn parse_embed_url_from_otakudesu_js_wrapper() {
        let html = r#"otakudesu('{"file":"https:\/\/cdn.desustream.net\/v.mp4?a=1&amp;b=2"}')"#;
        assert_eq!(
            parse_embed_url(html).unwrap(),
            "https://cdn.desustream.net/v.mp4?a=1&b=2"
        );
    }

    #[test]
    fn parse_embed_url_from_plain_file_key() {
        let html = r#"jwplayer("player").setup({"file":"https://cdn.x.com/v.mp4?a=1&amp;b=2"});"#;
        assert_eq!(
            parse_embed_url(html).unwrap(),
            "https://cdn.x.com/v.mp4?a=1&b=2"
        );
    }

    #[test]
    fn parse_embed_url_returns_none_when_absent() {
        assert_eq!(parse_embed_url("<html>tidak ada video di sini</html>"), None);
    }

    #[test]
    fn unpack_packed_js_decodes_tokens() {
        let mut words: Vec<&str> = vec![""; 52];
        words[11] = "var"; // token "b" (base 36)
        words[50] = "https"; // token "1e"
        words[51] = "hls"; // token "1f"
        let dict = words.join("|");
        let text = format!(
            "eval(function(p,a,c,k,e,d){{while(c--)if(k[c])p=p.replace(new RegExp('x'),'y');return p}}('b x={{\\\"1f\\\":\\\"1e\\\"}};',36,52,'{}'.split('|')))",
            dict
        );
        let out = unpack_packed_js(&text).unwrap();
        assert_eq!(out, "var x={\"hls\":\"https\"};");
    }

    #[test]
    fn unpack_packed_js_returns_none_for_plain_html() {
        assert_eq!(unpack_packed_js("<html>biasa saja</html>"), None);
    }

    #[test]
    fn parse_vidhide_url_prefers_hls4_and_absolutizes() {
        let js = r#"var links={"hls4":"/stream/abc/master.m3u8","hls3":"https://cdn.x/master.txt","hls2":"https://cdn.y/master.m3u8"};"#;
        assert_eq!(
            parse_vidhide_url(js, "https://odvidhide.com").unwrap(),
            "https://odvidhide.com/stream/abc/master.m3u8"
        );
    }

    #[test]
    fn parse_vidhide_url_falls_back_to_hls2_when_hls4_absent() {
        let js = r#"var links={"hls3":"https://cdn.x/master.txt","hls2":"https://cdn.y/master.m3u8?t=1"};"#;
        assert_eq!(
            parse_vidhide_url(js, "https://odvidhide.com").unwrap(),
            "https://cdn.y/master.m3u8?t=1"
        );
        assert_eq!(parse_vidhide_url("var x = 1;", "https://o.com"), None);
    }

    #[test]
    fn looks_like_media_url_detects_direct_streams() {
        assert!(looks_like_media_url("https://cdn.x/v.mp4"));
        assert!(looks_like_media_url("https://hls.y/master.m3u8?token=1"));
        assert!(looks_like_media_url("https://rr1.googlevideo.com/videoplayback?x=1"));
        assert!(!looks_like_media_url("https://odvidhide.com/embed/bx0ljgg81ofp"));
    }
}
