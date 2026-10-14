use async_trait::async_trait;

use crate::models::{Episode, Meta, Movie, Stream};

pub mod otakudesu;
pub mod idlix;
pub mod samehadaku;
pub mod kusonime;

// `async_trait` menambahkan `#[must_use]` tanpa pesan pada method yang hasilnya
// sudah `#[must_use]` (Result + Future) → clippy::double_must_use.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait Ext {
    async fn search(&mut self, title: String, page: usize) -> anyhow::Result<(Vec<Movie>, u64)>;
    async fn get_episodes(&self, movie: Movie) -> anyhow::Result<(Vec<Episode>, Meta)>;
    async fn get_stream_urls(&self, episode: Episode) -> anyhow::Result<Vec<Stream>>;

    /// Whether this provider actually honours the `page` argument in `search`.
    /// UI only shows next/prev buttons when this returns `true`.
    fn supports_pagination(&self) -> bool {
        false
    }
}
