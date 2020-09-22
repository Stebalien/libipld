//! Store traits.
use crate::block::Block;
use crate::cid::Cid;
use crate::codec::{Codec, Decode};
use crate::error::Result;
use crate::ipld::Ipld;
use crate::multihash::MultihashDigest;
use crate::path::DagPath;
use async_trait::async_trait;

/// The store parameters.
pub trait StoreParams: Clone + Send + Sync + Unpin + 'static {
    /// The multihash type of the store.
    type Hashes: MultihashDigest;
    /// The codec type of the store.
    type Codecs: Codec;
    /// The maximum block size supported by the store.
    const MAX_BLOCK_SIZE: usize;
}

/// Default store parameters.
#[derive(Clone)]
pub struct DefaultStoreParams;

impl StoreParams for DefaultStoreParams {
    const MAX_BLOCK_SIZE: usize = usize::MAX;
    type Codecs = crate::Multicodec;
    type Hashes = crate::multihash::Multihash;
}

/// Implementable by ipld stores. An ipld store behaves like a cache. It will keep blocks
/// until the cache is full after which it evicts blocks based on an eviction policy. If
/// a block is aliased (recursive named pin), it and it's recursive references will not
/// be evicted or counted towards the cache size.
#[async_trait]
pub trait Store: Clone + Send + Sync {
    /// Store parameters.
    type Params: StoreParams;

    /// Returns a block from the store. If the store supports networking and the block is not
    /// in the store it fetches it from the network and inserts it into the store. Dropping the
    /// future cancels the request.
    ///
    /// If the block wasn't found it returns a `BlockNotFound` error.
    async fn get(&self, cid: &Cid) -> Result<Block<Self::Params>>;

    /// Inserts a block into the store and publishes the block on the network.
    async fn insert(&self, block: &Block<Self::Params>) -> Result<()>;

    /// Resolves a path recursively and returns the ipld.
    async fn query(&self, path: &DagPath<'_>) -> Result<Ipld>
    where
        Ipld: Decode<<Self::Params as StoreParams>::Codecs>,
    {
        let mut root = self.get(path.root()).await?.ipld()?;
        let mut ipld = &root;
        for segment in path.path().iter() {
            ipld = ipld.get(segment)?;
            if let Ipld::Link(cid) = ipld {
                root = self.get(cid).await?.ipld()?;
                ipld = &root;
            }
        }
        Ok(ipld.clone())
    }

    /// Creates an alias for a `Cid`. To alias a block all it's recursive references
    /// must be in the store. If blocks are missing, they will be fetched from the network. If
    /// they aren't found, it will return a `BlockNotFound` error.
    async fn alias<T: AsRef<[u8]> + Send + Sync>(&self, alias: T, cid: Option<&Cid>) -> Result<()>;

    /// Resolves an alias for a `Cid`.
    async fn resolve<T: AsRef<[u8]> + Send + Sync>(&self, alias: T) -> Result<Option<Cid>>;
}

/// Creates a static alias concatenating the module path with an identifier.
#[macro_export]
macro_rules! alias {
    ($name:ident) => {
        concat!(module_path!(), "::", stringify!($name))
    };
}

/// Creates a dynamic alias by appending a id.
pub fn dyn_alias(alias: &'static str, id: u64) -> String {
    let mut alias = alias.to_string();
    alias.push_str("::");
    alias.push_str(&id.to_string());
    alias
}

#[cfg(test)]
mod tests {
    use super::*;

    mod aliases {
        pub const CHAIN_ALIAS: &str = alias!(CHAIN_ALIAS);
    }

    #[test]
    fn test_alias() {
        assert_eq!(alias!(test_alias), "libipld::store::tests::test_alias");
        assert_eq!(
            aliases::CHAIN_ALIAS,
            "libipld::store::tests::aliases::CHAIN_ALIAS"
        );
        assert_eq!(
            dyn_alias(aliases::CHAIN_ALIAS, 3).as_str(),
            "libipld::store::tests::aliases::CHAIN_ALIAS::3"
        );
    }
}
