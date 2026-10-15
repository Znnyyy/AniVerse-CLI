use base64::prelude::*;
use std::collections::HashMap;
use async_trait::async_trait;
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

    /// Ambil URL video langsung dari halaman embed (desustream).
    /// Mengembalikan `None` jika halaman embed ternyata maintenance / bukan video,
    /// agar URL halaman HTML tidak sampai dikirim ke pemutar.
    async fn resolve_stream_url(&self, page_url: &str) -> Option<String> {
        if page_url.contains("/maintenance") {
            return None;
        }
        if !page_url.contains("desustream") {
            return Some(clean_url(page_url));
        }
        let Ok(resp) = self.client.get(page_url).send().await else {
            return Some(clean_url(page_url));
        };
        if resp.url().path().contains("/maintenance") {
            return None;
        }
        let Ok(text) = resp.text().await else {
            return Some(clean_url(page_url));
        };
        match parse_embed_url(&text) {
            Some(url) => Some(url),
            None if text.to_lowercase().contains("maintenance") => None,
            None => Some(clean_url(page_url)),
        }
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
}
