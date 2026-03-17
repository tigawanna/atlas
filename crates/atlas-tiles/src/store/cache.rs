use std::num::NonZeroUsize;
use std::sync::Arc;

use async_trait::async_trait;
use lru::LruCache;
use tokio::sync::Mutex;

use super::{TileResponse, TileStore};
use atlas_core::{AtlasError, TileCoord};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    tileset: String,
    z: u8,
    x: u32,
    y: u32,
}

pub struct CachedTileStore {
    inner: Arc<dyn TileStore>,
    cache: Mutex<LruCache<CacheKey, TileResponse>>,
}

impl CachedTileStore {
    pub fn new(inner: Arc<dyn TileStore>, max_entries: usize) -> Self {
        let cap = NonZeroUsize::new(max_entries.max(1)).unwrap();
        Self {
            inner,
            cache: Mutex::new(LruCache::new(cap)),
        }
    }
}

#[async_trait]
impl TileStore for CachedTileStore {
    async fn get_tile(
        &self,
        tileset: &str,
        coord: TileCoord,
    ) -> Result<Option<TileResponse>, AtlasError> {
        let key = CacheKey {
            tileset: tileset.to_string(),
            z: coord.z,
            x: coord.x,
            y: coord.y,
        };

        {
            let mut cache = self.cache.lock().await;
            if let Some(cached) = cache.get(&key) {
                metrics::counter!("atlas_tile_cache_hits_total").increment(1);
                return Ok(Some(cached.clone()));
            }
        }

        let result = self.inner.get_tile(tileset, coord).await?;

        if let Some(ref tile) = result {
            let mut cache = self.cache.lock().await;
            cache.put(key, tile.clone());
        }

        metrics::counter!("atlas_tile_cache_misses_total").increment(1);

        Ok(result)
    }

    async fn get_tilejson(
        &self,
        tileset: &str,
        public_url: &str,
    ) -> Result<tilejson::TileJSON, AtlasError> {
        self.inner.get_tilejson(tileset, public_url).await
    }

    fn tilesets(&self) -> Vec<String> {
        self.inner.tilesets()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::TileFormat;
    use bytes::Bytes;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct MockStore {
        call_count: AtomicU32,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                call_count: AtomicU32::new(0),
            }
        }
    }

    #[async_trait]
    impl TileStore for MockStore {
        async fn get_tile(
            &self,
            _tileset: &str,
            _coord: TileCoord,
        ) -> Result<Option<TileResponse>, AtlasError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(Some(TileResponse {
                data: Bytes::from_static(b"tile-data"),
                format: TileFormat::Mvt,
            }))
        }

        async fn get_tilejson(
            &self,
            _tileset: &str,
            _public_url: &str,
        ) -> Result<tilejson::TileJSON, AtlasError> {
            Ok(tilejson::tilejson! { tiles: vec![] })
        }

        fn tilesets(&self) -> Vec<String> {
            vec!["test".to_string()]
        }
    }

    #[tokio::test]
    async fn cache_hit_avoids_inner_call() {
        let mock = Arc::new(MockStore::new());
        let cached = CachedTileStore::new(mock.clone(), 100);
        let coord = TileCoord::new(0, 0, 0).unwrap();

        let _ = cached.get_tile("test", coord).await.unwrap();
        let _ = cached.get_tile("test", coord).await.unwrap();

        assert_eq!(mock.call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cache_miss_calls_inner() {
        let mock = Arc::new(MockStore::new());
        let cached = CachedTileStore::new(mock.clone(), 100);

        let c1 = TileCoord::new(0, 0, 0).unwrap();
        let c2 = TileCoord::new(1, 0, 0).unwrap();

        let _ = cached.get_tile("test", c1).await.unwrap();
        let _ = cached.get_tile("test", c2).await.unwrap();

        assert_eq!(mock.call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn eviction_when_full() {
        let mock = Arc::new(MockStore::new());
        let cached = CachedTileStore::new(mock.clone(), 1);

        let c1 = TileCoord::new(1, 0, 0).unwrap();
        let c2 = TileCoord::new(1, 1, 0).unwrap();

        let _ = cached.get_tile("test", c1).await.unwrap();
        let _ = cached.get_tile("test", c2).await.unwrap();
        let _ = cached.get_tile("test", c1).await.unwrap();

        assert_eq!(mock.call_count.load(Ordering::SeqCst), 3);
    }
}
