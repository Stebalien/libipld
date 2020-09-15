//! Cache
use crate::cid::Cid;
use crate::codec::{Codec, Decode, Encode};
use crate::error::Result;
use crate::ipld::Ipld;
use crate::store::{Store, StoreParams, Transaction as RawTransaction};
use async_std::sync::Mutex;
use async_trait::async_trait;
use cached::stores::SizedCache;
use cached::Cached;
use std::borrow::Cow;

/// Typed transaction.
pub struct Transaction<'a, S: StoreParams, C, T> {
    codec: C,
    hash: u64,
    tx: RawTransaction<'a, S>,
    cache: Vec<(Cid, T)>,
}

impl<'a, S, C, T> Transaction<'a, S, C, T>
where
    S: StoreParams,
    C: Codec + Into<S::Codecs>,
    T: Decode<C> + Encode<C> + Clone + Send + Sync,
    Ipld: Decode<S::Codecs>,
{
    /// Creates a new transaction.
    pub fn new(codec: C, hash: u64) -> Self {
        Self {
            codec,
            hash,
            tx: RawTransaction::new(),
            cache: Vec::new(),
        }
    }

    /// Creates a new batch with capacity.
    pub fn with_capacity(codec: C, hash: u64, capacity: usize) -> Self {
        Self {
            codec,
            hash,
            tx: RawTransaction::with_capacity(capacity),
            cache: Vec::with_capacity(capacity),
        }
    }

    /// Inserts a value into the batch.
    pub fn insert(&mut self, value: T) -> Result<Cid> {
        let cid = self.tx.create(self.codec, self.hash, &value)?;
        self.cache.push((cid.clone(), value));
        Ok(cid)
    }

    /// Pins a block.
    pub fn pin<I: Into<Cow<'a, Cid>>>(&mut self, cid: I) {
        self.tx.pin(cid.into());
    }

    /// Pins a block.
    pub fn unpin<I: Into<Cow<'a, Cid>>>(&mut self, cid: I) {
        self.tx.unpin(cid.into());
    }

    /// Updates a block.
    pub fn update<I: Into<Cow<'a, Cid>>, N: Into<Cow<'a, Cid>>>(&mut self, old: Option<I>, new: N) {
        let old = old.map(|val| val.into());
        self.tx.update(old, new.into());
    }
}

/// Cache for ipld blocks.
pub struct IpldCache<S, C, T> {
    store: S,
    codec: C,
    hash: u64,
    cache: Mutex<SizedCache<Cid, T>>,
}

impl<S, C, T> IpldCache<S, C, T> {
    /// Creates a new cache of size `size`.
    pub fn new(store: S, codec: C, hash: u64, size: usize) -> Self {
        let cache = Mutex::new(SizedCache::with_size(size));
        Self {
            store,
            codec,
            hash,
            cache,
        }
    }
}

/// Cache trait.
#[async_trait]
pub trait Cache<S, C, T>
where
    S: StoreParams,
    S::Codecs: Into<C>,
    C: Codec + Into<S::Codecs>,
    T: Decode<C> + Encode<C> + Clone + Send + Sync,
{
    /// Creates a transaction.
    fn transaction(&self) -> Transaction<'_, S, C, T>;

    /// Creates a transaction with capacity.
    fn transaction_with_capacity(&self, capacity: usize) -> Transaction<'_, S, C, T>;

    /// Returns a decoded block.
    async fn get(&self, cid: &Cid) -> Result<T>;

    /// Commits a transaction.
    async fn commit(&self, tx: Transaction<'_, S, C, T>) -> Result<()>;
}

#[async_trait]
impl<S, C, T> Cache<S::Params, C, T> for IpldCache<S, C, T>
where
    S: Store,
    <S::Params as StoreParams>::Codecs: Into<C>,
    C: Codec + Into<<S::Params as StoreParams>::Codecs>,
    T: Decode<C> + Encode<C> + Clone + Send + Sync,
    Ipld: Decode<<S::Params as StoreParams>::Codecs>,
{
    fn transaction(&self) -> Transaction<'_, S::Params, C, T> {
        Transaction::new(self.codec, self.hash)
    }

    fn transaction_with_capacity(&self, capacity: usize) -> Transaction<'_, S::Params, C, T> {
        Transaction::with_capacity(self.codec, self.hash, capacity)
    }

    async fn get(&self, cid: &Cid) -> Result<T> {
        if let Some(value) = self.cache.lock().await.cache_get(cid).cloned() {
            return Ok(value);
        }
        let block = self.store.get(cid).await?;
        let value: T = block.decode::<C, _>()?;
        let (cid, _) = block.into_inner();
        self.cache.lock().await.cache_set(cid, value.clone());
        Ok(value)
    }

    async fn commit(&self, transaction: Transaction<'_, S::Params, C, T>) -> Result<()> {
        self.store.commit(transaction.tx).await?;
        let mut cache = self.cache.lock().await;
        for (cid, value) in transaction.cache {
            cache.cache_set(cid, value);
        }
        Ok(())
    }
}

/// Macro to derive cache trait for a struct.
#[macro_export]
macro_rules! derive_cache {
    ($struct:tt, $field:ident, $codec:ty, $type:ty) => {
        #[async_trait::async_trait]
        impl<S> $crate::cache::Cache<S::Params, $codec, $type> for $struct<S>
        where
            S: $crate::store::Store,
            <S::Params as $crate::store::StoreParams>::Codecs: From<$codec> + Into<$codec>,
            Ipld: $crate::codec::Decode<<S::Params as $crate::store::StoreParams>::Codecs>,
        {
            fn transaction(&self) -> $crate::cache::Transaction<'_, S::Params, $codec, $type> {
                self.$field.transaction()
            }

            fn transaction_with_capacity(
                &self,
                capacity: usize,
            ) -> $crate::cache::Transaction<'_, S::Params, $codec, $type> {
                self.$field.transaction_with_capacity(capacity)
            }

            async fn get(&self, cid: &$crate::cid::Cid) -> $crate::error::Result<$type> {
                self.$field.get(cid).await
            }

            async fn commit(
                &self,
                tx: $crate::cache::Transaction<'_, S::Params, $codec, $type>,
            ) -> $crate::error::Result<()> {
                self.$field.commit(tx).await
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbor::DagCborCodec;
    use crate::mem::MemStore;
    use crate::multihash::BLAKE2B_256;
    use crate::store::DefaultStoreParams;
    use core::ops::Deref;

    struct OffchainClient<S> {
        store: S,
        number: IpldCache<S, DagCborCodec, u32>,
    }

    impl<S> Deref for OffchainClient<S> {
        type Target = S;

        fn deref(&self) -> &Self::Target {
            &self.store
        }
    }

    derive_cache!(OffchainClient, number, DagCborCodec, u32);

    #[async_std::test]
    async fn test_cache() {
        let store = MemStore::<DefaultStoreParams>::default();
        let client = OffchainClient {
            store: store.clone(),
            number: IpldCache::new(store, DagCborCodec, BLAKE2B_256, 1),
        };
        let mut tx = client.transaction_with_capacity(2);
        let cid = tx.insert(42).unwrap();
        tx.pin(&cid);
        client.commit(tx).await.unwrap();

        let res = client.get(&cid).await.unwrap();
        assert_eq!(res, 42);
        client.unpin(cid).await.unwrap();
    }
}
