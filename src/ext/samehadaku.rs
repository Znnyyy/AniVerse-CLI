use anyhow::Context;
use async_trait::async_trait;
use reqwest::Client;

use crate::{
    ext::Ext,
    models::{Episode, Meta, Movie, Stream},
};

const BASE_URL: &str = "https://samehadaku.li";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub struct Samehadaku {
    client: Client,
}

impl Samehadaku {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .unwrap(),
        }
    }
}

/// Ekstrak daftar link episode dari halaman anime.
/// Mengembalikan (nomor_episode, url, judul_tautan).
fn parse_episode_links(html: &str) -> Vec<(u32, String, String)> {
    let re = crate::regex!(
        r#"(?i)<a[^>]+href="(https://samehadaku\.li/[^"]+-episode-(\d+)-subtitle-indonesia/)"[^>]*>([\s\S]*?)</a>"#
    );
    let mut seen: Vec<u32> = Vec::new();
    let mut out: Vec<(u32, String, String)> = Vec::new();
    let tag_re = crate::regex!(r"<[^>]+>");
    for caps in re.captures_iter(html) {
        let url = caps[1].to_string();
        let num: u32 = match caps[2].parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if seen.contains(&num) {
            continue;
        }
        seen.push(num);
        let label = tag_re.replace_all(&caps[3], " ");
        let label = html_escape::decode_html_entities(&label);
        let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
        let title = if label.is_empty() {
            format!("Episode {}", num)
        } else {
            label
        };
        out.push((num, url, title));
    }
    out.sort_by_key(|(num, _, _)| std::cmp::Reverse(*num));
    out
}

/// Skor kualitas per itag Blogger (yang umum: 13/5/6/18/22/37).
fn itag_score(itag: u32) -> u32 {
    match itag {
        13 => 144,
        5 => 240,
        6 | 18 => 360,
        22 => 720,
        37 => 1080,
        _ => 0,
    }
}

/// Ambil URL googlevideo terbaik dari balasan RPC batchexecute Blogger.
/// Balasan memuat URL ter-escape (`\\u003d` dsb.) di dalam string JSON.
fn best_stream_from_rpc(resp: &str) -> Option<String> {
    let re_url = crate::regex!(r#"https?:[^"]*videoplayback[^"]*"#);
    let re_itag = crate::regex!(r"itag=(\d+)");
    let re_u003d = crate::regex!(r"\\+u003d");
    let re_u0026 = crate::regex!(r"\\+u0026");
    let re_u002f = crate::regex!(r"\\+u002f");

    let mut best: Option<(u32, u32, String)> = None; // (skor, itag, url)
    let mut seen: Vec<String> = Vec::new();
    for m in re_url.find_iter(resp) {
        let raw = m.as_str().trim_end_matches('\\');
        let url = re_u003d.replace_all(raw, "=");
        let url = re_u0026.replace_all(&url, "&");
        let url = re_u002f.replace_all(&url, "/").to_string();
        if seen.contains(&url) {
            continue;
        }
        seen.push(url.clone());
        let itag: u32 = match re_itag.captures(&url) {
            Some(c) => c[1].parse().unwrap_or(0),
            None => 0,
        };
        let score = itag_score(itag);
        if score == 0 {
            continue; // tanpa/itag tak dikenal = noise (audio-only dsb.)
        }
        if best.as_ref().is_none_or(|(s, _, _)| score > *s) {
            best = Some((score, itag, url));
        }
    }
    best.map(|(_, _, url)| url)
}

/// Resolver token/URL video.g Blogger → URL googlevideo langsung.
/// Alur: halaman token → sid (kunci FdrFJe) & bl (kunci cfb2h) → RPC batchexecute WcwnYd.
/// URL googlevideo terikat IP: harus di-resolve dari IP yang sama yang memutar.
async fn resolve_blogger_url(client: &Client, embed_url: &str) -> anyhow::Result<String> {
    let token = embed_url
        .split("token=")
        .nth(1)
        .unwrap_or("")
        .split('&')
        .next()
        .unwrap_or("");
    anyhow::ensure!(!token.is_empty(), "token Blogger tidak ditemukan di URL");

    let page = client
        .get(format!("https://www.blogger.com/video.g?token={}", token))
        .send()
        .await?
        .text()
        .await?;
    let extract_key = |key: &str| -> Option<String> {
        let pat = format!("{}\":\"", key);
        page.split(&pat)
            .nth(1)
            .and_then(|s| s.split('"').next())
            .map(|s| s.to_string())
    };
    let sid = extract_key("FdrFJe").context("sid (FdrFJe) tidak ada di halaman token")?;
    let bl = extract_key("cfb2h").context("bl (cfb2h) tidak ada di halaman token")?;

    let reqid = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        % 86400;
    let rpc_url = format!(
        "https://www.blogger.com/_/BloggerVideoPlayerUi/data/batchexecute\
         ?rpcids=WcwnYd&source-path=/video.g&f.sid={}&bl={}&hl=en-US&_reqid={}&rt=c",
        sid, bl, reqid
    );
    let f_req = format!(
        "[[[\"WcwnYd\",\"[\\\"{}\\\",null,0]\",null,\"generic\"]]]",
        token
    );
    let resp = client
        .post(&rpc_url)
        .header(
            "Content-Type",
            "application/x-www-form-urlencoded;charset=UTF-8",
        )
        .header("X-Same-Domain", "1")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", "https://www.blogger.com/")
        .form(&[("f.req", f_req.as_str())])
        .send()
        .await?
        .text()
        .await?;
    anyhow::ensure!(resp.contains("videoplayback"), "RPC tidak mengembalikan stream");
    best_stream_from_rpc(&resp).context("tidak ada URL videoplayback yang valid di balasan RPC")
}

/// Coba resolve URL video (mis. halaman embed) menjadi URL langsung via yt-dlp.
fn resolve_playable_url(url: &str) -> String {
    if !crate::check_command_exists("yt-dlp") && !crate::check_command_exists("youtube-dl") {
        return url.to_string();
    }
    let bin = if crate::check_command_exists("yt-dlp") {
        crate::resolve_command_path("yt-dlp")
    } else {
        crate::resolve_command_path("youtube-dl")
    };
    let out = std::process::Command::new(bin)
        .args(["-g", "-f", "bv*+ba/b/best"])
        .arg(url)
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .lines()
                .map(str::trim)
                .find(|l| l.starts_with("http"))
                .map(|s| s.to_string())
                .unwrap_or_else(|| url.to_string())
        }
        _ => url.to_string(),
    }
}

#[async_trait]
impl Ext for Samehadaku {
    async fn search(&mut self, title: String, page: usize) -> anyhow::Result<(Vec<Movie>, u64)> {
        let query = urlencoding::encode(&title);
        // Pencarian lewat WP REST API (pencarian HTML situs ini memblokir bot).
        let url = format!(
            "{}/wp-json/wp/v2/search?search={}&per_page=20&page={}",
            BASE_URL, query, page
        );
        let items: Vec<serde_json::Value> =
            self.client.get(&url).send().await?.json().await.unwrap_or_default();

        let mut movies = vec![];
        for item in &items {
            let item_url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if !item_url.contains("/anime/") {
                continue;
            }
            let item_title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if item_title.is_empty() {
                continue;
            }
            movies.push(Movie {
                id: item_url.trim_end_matches('/').to_string(),
                title: item_title,
                total_episodes: None,
            });
        }
        let total = items.len() as u64;
        Ok((movies, total))
    }

    async fn get_episodes(&self, movie: Movie) -> anyhow::Result<(Vec<Episode>, Meta)> {
        let page_url = format!("{}/", movie.id.trim_end_matches('/'));
        let html = self.client.get(&page_url).send().await?.text().await?;

        let episodes: Vec<Episode> = parse_episode_links(&html)
            .into_iter()
            .map(|(_, url, title)| Episode {
                id: url,
                title,
                is_series: true,
            })
            .collect();

        // Poster + info minimal dari halaman anime.
        let re_thumb = crate::regex!(r#"(?i)<meta[^>]+property="og:image"[^>]+content="([^"]+)""#);
        let thumb_url = re_thumb
            .captures(&html)
            .map(|c| html_escape::decode_html_entities(&c[1]).to_string());

        let mut data: Vec<(String, String)> = vec![];
        let re_status = crate::regex!(r"(?i)Status:\s*([A-Za-z]+)");
        if let Some(c) = re_status.captures(&html) {
            data.push(("Status".to_string(), c[1].to_string()));
        }
        let re_type = crate::regex!(r"(?i)Type:\s*([A-Za-z0-9]+)");
        if let Some(c) = re_type.captures(&html) {
            data.push(("Type".to_string(), c[1].to_string()));
        }

        Ok((
            episodes,
            Meta {
                thumb_url,
                data,
            },
        ))
    }

    async fn get_stream_urls(&self, episode: Episode) -> anyhow::Result<Vec<Stream>> {
        let html = self.client.get(&episode.id).send().await?.text().await?;

        let re_iframe =
            crate::regex!(r#"(?i)<iframe[^>]+src="(https?://[^"]+)"[^>]*class="[^"]*player-embed[^"]*"|<div[^>]+class="[^"]*player-embed[^"]*"[^>]*>\s*<iframe[^>]+src="(https?://[^"]+)""#);
        // Fallback: cari iframe pertama di area player-embed bila pola kompleks tidak cocok.
        let re_any_iframe = crate::regex!(r#"(?i)player-embed[\s\S]{0,400}?<iframe[^>]+src="(https?://[^"]+)""#);

        let mut streams = vec![];
        let mut embed_url: Option<String> = None;
        if let Some(c) = re_iframe.captures(&html) {
            let u = c.get(1).or_else(|| c.get(2)).map(|m| m.as_str().to_string());
            embed_url = u;
        }
        if embed_url.is_none() {
            embed_url = re_any_iframe
                .captures(&html)
                .map(|c| c[1].to_string());
        }

        if let Some(url) = embed_url {
            let raw = html_escape::decode_html_entities(&url).to_string();
            // Embed Blogger: yt-dlp gagal (player baru), jadi resolve via RPC sendiri;
            // yt-dlp tetap dipakai sebagai cadangan untuk jenis embed lain.
            let resolved = if raw.contains("blogger.com/video.g") {
                match resolve_blogger_url(&self.client, &raw).await {
                    Ok(u) => u,
                    Err(_) => resolve_playable_url(&raw),
                }
            } else {
                resolve_playable_url(&raw)
            };
            let title = if resolved == raw {
                "Default (embed)".to_string()
            } else {
                "Default".to_string()
            };
            streams.push(Stream {
                url: resolved,
                title,
            });
        }

        // Link unduhan sebagai cadangan (biasanya Google Drive / Mirror).
        let re_dl = crate::regex!(
            r#"(?i)href="(https?://(?:drive\.google\.com|mega\.nz|terabox\.com|fex\.net)[^"]+)""#
        );
        let mut seen: Vec<String> = Vec::new();
        for caps in re_dl.captures_iter(&html) {
            let url = html_escape::decode_html_entities(&caps[1]).to_string();
            if seen.contains(&url) {
                continue;
            }
            seen.push(url.clone());
            let host = if url.contains("drive.google") {
                "Google Drive"
            } else if url.contains("mega.nz") {
                "Mega"
            } else {
                "Mirror"
            };
            streams.push(Stream {
                url,
                title: format!("Download - {}", host),
            });
        }

        Ok(streams)
    }

    fn supports_pagination(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_episode_links_sorts_desc_and_dedupes() {
        let html = r#"
        <div class="episodelist">
          <li><a href="https://samehadaku.li/one-piece-episode-100-subtitle-indonesia/"><span class="epcur">Episode 100</span></a></li>
          <li><a href="https://samehadaku.li/one-piece-episode-102-subtitle-indonesia/"><span class="epcur">Episode 102</span></a></li>
          <li><a href="https://samehadaku.li/one-piece-episode-100-subtitle-indonesia/">duplikat</a></li>
          <li><a href="https://samehadaku.li/one-piece-episode-99-subtitle-indonesia/"><span class="epcur">Episode 99</span></a></li>
        </div>"#;
        let eps = parse_episode_links(html);
        assert_eq!(eps.len(), 3);
        assert_eq!(eps[0].0, 102);
        assert_eq!(eps[1].0, 100);
        assert_eq!(eps[2].0, 99);
        assert!(eps[0].1.ends_with("episode-102-subtitle-indonesia/"));
        assert!(eps[0].2.contains("Episode 102"));
    }

    #[test]
    fn parse_episode_links_empty_when_no_match() {
        assert!(parse_episode_links("<html>halaman kosong</html>").is_empty());
    }

    #[test]
    fn best_stream_from_rpc_unescapes_and_picks_highest_itag() {
        let resp = r#")]}'\n\n[[["WcwnYd","[\"tok\",null,0]"]]]
{"1":null,"2":"https://redirector.googlevideo.com/videoplayback?expire=1\\u0026itag=18\\u0026mime=video%2Fmp4\\"}
{"1":null,"2":"https://redirector.googlevideo.com/videoplayback?expire=1\\u0026itag=22\\u0026mime=video%2Fmp4\\"}
{"1":null,"2":"https://redirector.googlevideo.com/videoplayback?expire=1\\u0026mime=audio%2Fmp4\\"}
"#;
        let best = best_stream_from_rpc(resp).expect("harus menemukan stream");
        assert!(best.contains("itag=22"), "harus memilih itag 22: {}", best);
        assert!(best.contains("&mime="), "u0026 harus di-unescape: {}", best);
        assert!(!best.contains("u0026"), "tidak boleh sisa escape: {}", best);
    }

    #[test]
    fn best_stream_from_rpc_none_when_no_streams() {
        assert!(best_stream_from_rpc(")]}'\n\nerror biasa").is_none());
    }

    #[tokio::test]
    #[ignore = "membutuhkan jaringan"]
    async fn live_samehadaku_full_flow() {
        let mut provider = Samehadaku::new();
        let (movies, _) = provider
            .search("one piece".into(), 1)
            .await
            .expect("search harus sukses");
        assert!(!movies.is_empty(), "search harus menemukan judul");

        let (episodes, meta) = provider
            .get_episodes(movies[0].clone())
            .await
            .expect("get_episodes harus sukses");
        assert!(!episodes.is_empty(), "harus menemukan episode");
        println!(
            "movie: {} | thumb: {:?} | ep pertama: {}",
            movies[0].title, meta.thumb_url, episodes[0].title
        );

        let streams = provider
            .get_stream_urls(episodes[0].clone())
            .await
            .expect("get_stream_urls harus sukses");
        assert!(!streams.is_empty(), "harus menemukan stream");
        for s in &streams {
            println!("stream: {} -> {}", s.title, &s.url[..s.url.len().min(90)]);
        }

        // Embed Blogger harus ter-resolve menjadi URL googlevideo langsung.
        let gv = streams
            .iter()
            .find(|s| s.url.contains("googlevideo.com"))
            .expect("harus ada stream googlevideo hasil resolve Blogger");
        println!("googlevideo OK: {}...", &gv.url[..gv.url.len().min(100)]);

        // Probe rentang awal — memastikan URL benar-benar bisa di-stream dari IP ini.
        let probe = provider
            .client
            .get(&gv.url)
            .header("Range", "bytes=0-1023")
            .header("Referer", "https://www.blogger.com/")
            .send()
            .await
            .expect("probe harus sukses");
        let status = probe.status();
        println!("probe status: {}", status);
        assert!(
            status.as_u16() == 200 || status.as_u16() == 206,
            "probe stream harus 200/206, dapat {}",
            status
        );
    }
}
