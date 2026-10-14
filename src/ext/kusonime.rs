use async_trait::async_trait;
use reqwest::Client;

use crate::{
    ext::Ext,
    models::{Episode, Meta, Movie, Stream},
};

const BASE_URL: &str = "https://kusonime.com";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub struct Kusonime {
    client: Client,
    /// URL hasil pertama di halaman 1 — dipakai untuk mendeteksi pagination mentok.
    page1_first_url: Option<String>,
}

impl Kusonime {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .unwrap(),
            page1_first_url: None,
        }
    }
}

/// Parse hasil pencarian Kusonime (link post di heading h2/h3).
/// Mengembalikan (url, judul) unik, tanpa link non-post.
fn parse_search_results(html: &str) -> Vec<(String, String)> {
    let re = crate::regex!(
        r#"(?i)<h[23][^>]*>\s*<a[^>]+href="(https://kusonime\.com/[^"]+)"[^>]*>([^<]+)</a>"#
    );
    let mut out: Vec<(String, String)> = Vec::new();
    for caps in re.captures_iter(html) {
        let url = caps[1].trim_end_matches('/').to_string();
        // Buang link navigasi/feed non-post.
        if url.contains("wp-content")
            || url.contains("/feed")
            || url.contains("/search/")
            || url.contains("list-anime")
            || url.contains("anime-list")
            || url.contains("anime-movie-list")
            || url.contains("daftar-live-action")
            || url.contains("/seasons/")
            || url.ends_with(".com")
        {
            continue;
        }
        if out.iter().any(|(u, _)| *u == url) {
            continue;
        }
        let title = html_escape::decode_html_entities(&caps[2]).trim().to_string();
        out.push((url, title));
    }
    out
}

/// Bersihkan judul post Kusonime agar enak ditampilkan sebagai judul episode.
fn clean_post_title(title: &str) -> String {
    let mut t = html_escape::decode_html_entities(title).trim().to_string();
    t = t.replace("Subtitle Indonesia", "");
    t = t.replace("subtitle indonesia", "");
    let re_spaces = crate::regex!(r"\s+");
    re_spaces.replace_all(&t, " ").trim().to_string()
}

/// Parse link unduhan (Google Drive / Mega / mirror lain) dari halaman post.
fn parse_download_links(html: &str) -> Vec<String> {
    let re = crate::regex!(
        r#"(?i)href="(https?://(?:drive\.google\.com|mega\.nz|terabox\.com|fex\.net|mirror\.media)[^"]+)""#
    );
    let mut out: Vec<String> = Vec::new();
    for caps in re.captures_iter(html) {
        let url = html_escape::decode_html_entities(&caps[1]).to_string();
        if !out.contains(&url) {
            out.push(url);
        }
    }
    out
}

#[async_trait]
impl Ext for Kusonime {
    async fn search(&mut self, title: String, page: usize) -> anyhow::Result<(Vec<Movie>, u64)> {
        let query = urlencoding::encode(&title);
        let url = if page > 1 {
            format!("{}/page/{}/?s={}", BASE_URL, page, query)
        } else {
            self.page1_first_url = None;
            format!("{}/?s={}", BASE_URL, query)
        };
        let html = self.client.get(&url).send().await?.text().await?;
        let results = parse_search_results(&html);

        if page == 1 {
            self.page1_first_url = results.first().map(|(u, _)| u.clone());
        } else if let (Some(first), Some(page1)) = (results.first(), self.page1_first_url.as_ref()) {
            // Pagination mentok: WP mengulang halaman 1.
            if first.0 == *page1 {
                return Ok((vec![], 0));
            }
        }

        let total = results.len() as u64;
        let movies = results
            .into_iter()
            .map(|(url, title)| Movie {
                id: url,
                title,
                total_episodes: None,
            })
            .collect();
        Ok((movies, total))
    }

    async fn get_episodes(&self, movie: Movie) -> anyhow::Result<(Vec<Episode>, Meta)> {
        let html = self.client.get(&movie.id).send().await?.text().await?;

        // Kusonime berorientasi batch/movie: satu post = satu "episode".
        let re_title = crate::regex!(r#"(?i)<title>([^<]+)</title>"#);
        let title = re_title
            .captures(&html)
            .map(|c| clean_post_title(&c[1]))
            .unwrap_or_else(|| clean_post_title(&movie.title));

        let episodes = vec![Episode {
            id: movie.id.clone(),
            title,
            is_series: false,
        }];

        let re_thumb = crate::regex!(r#"(?i)<meta[^>]+property="og:image"[^>]+content="([^"]+)""#);
        let thumb_url = re_thumb
            .captures(&html)
            .map(|c| html_escape::decode_html_entities(&c[1]).to_string());

        let mut data: Vec<(String, String)> = vec![];
        if movie.title.to_lowercase().contains("batch") {
            data.push(("Jenis".to_string(), "Batch (kumpulan episode)".to_string()));
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
        let is_batch = episode.title.to_lowercase().contains("batch")
            || episode.id.to_lowercase().contains("batch");

        let mut streams = vec![];
        for (i, url) in parse_download_links(&html).into_iter().enumerate() {
            let host = if url.contains("drive.google") {
                "Google Drive"
            } else if url.contains("mega.nz") {
                "Mega"
            } else {
                "Mirror"
            };
            let batch_note = if is_batch { " (ZIP batch)" } else { "" };
            streams.push(Stream {
                url,
                title: format!("#{} - {}{}", i + 1, host, batch_note),
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
    fn parse_search_results_filters_nav_links() {
        let html = r#"
        <h2 class="entry-title"><a href="https://kusonime.com/boruto-batch-subtitle-indonesia-8/">Boruto Batch Subtitle Indonesia</a></h2>
        <h3><a href="https://kusonime.com/naruto-movie-sub-indo-9/">Naruto Movie Sub Indo</a></h3>
        <h3><a href="https://kusonime.com/list-anime-batch-sub-indo/">Daftar Batch</a></h3>
        <h3><a href="https://kusonime.com/feed/">Feed</a></h3>
        "#;
        let results = parse_search_results(html);
        assert_eq!(results.len(), 2);
        assert!(results[0].0.ends_with("boruto-batch-subtitle-indonesia-8"));
        assert_eq!(results[0].1, "Boruto Batch Subtitle Indonesia");
        assert!(results[1].0.contains("naruto-movie"));
    }

    #[test]
    fn clean_post_title_strips_subtitle_indonesia() {
        assert_eq!(
            clean_post_title("Naruto Movie 1 Subtitle Indonesia &#8211; BD"),
            "Naruto Movie 1 – BD"
        );
    }

    #[test]
    fn parse_download_links_dedupes_and_decodes() {
        let html = r#"
        <a href="https://drive.google.com/uc?id=abc&amp;export=download">GD</a>
        <a href="https://drive.google.com/uc?id=abc&export=download">GD lagi</a>
        <a href="https://mega.nz/#!xyz">Mega</a>
        <a href="https://example.com/ignored">Ignore</a>
        "#;
        let links = parse_download_links(html);
        assert_eq!(links.len(), 2);
        assert!(links[0].contains("id=abc"));
        assert!(!links[0].contains("&amp;"));
        assert!(links[1].starts_with("https://mega.nz/"));
    }

    #[tokio::test]
    #[ignore = "membutuhkan jaringan"]
    async fn live_kusonime_full_flow() {
        let mut provider = Kusonime::new();
        let (movies, _) = provider
            .search("naruto".into(), 1)
            .await
            .expect("search harus sukses");
        assert!(!movies.is_empty(), "search harus menemukan judul");

        let (episodes, _meta) = provider
            .get_episodes(movies[0].clone())
            .await
            .expect("get_episodes harus sukses");
        assert_eq!(episodes.len(), 1);

        let streams = provider
            .get_stream_urls(episodes[0].clone())
            .await
            .expect("get_stream_urls harus sukses");
        println!(
            "movie: {} | episode: {} | jumlah link: {}",
            movies[0].title,
            episodes[0].title,
            streams.len()
        );
    }
}
