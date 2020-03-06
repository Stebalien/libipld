//! Utilities for performing garbage collection.
use crate::error::BlockError;
use crate::hash::CidHashSet;
use crate::ipld::Ipld;
use crate::store::{Store, StoreIpldExt};

/// Returns the references in an ipld block.
pub fn references(ipld: &Ipld) -> CidHashSet {
    let mut set: CidHashSet = Default::default();
    for ipld in ipld.iter() {
        if let Ipld::Link(cid) = ipld {
            set.insert(cid.to_owned());
        }
    }
    set
}

/// Returns the recursive references of an ipld block.
pub async fn closure<TStore: Store>(
    store: &TStore,
    roots: CidHashSet,
) -> Result<CidHashSet, BlockError> {
    let mut stack = vec![roots];
    let mut set: CidHashSet = Default::default();
    while let Some(mut roots) = stack.pop() {
        for cid in roots.drain() {
            if set.contains(&cid) {
                continue;
            }
            if let Some(ipld) = store.read_ipld(&cid).await? {
                stack.push(references(&ipld));
            }
            set.insert(cid);
        }
    }
    Ok(set)
}

/// Returns the paths to gc.
///
/// This is currently not topologically sorted according to the references
/// relationship. (p < q if q.is_reference(p))
pub async fn dead_paths<TStore: Store>(
    store: &TStore,
    all_cids: CidHashSet,
    roots: CidHashSet,
) -> Result<CidHashSet, BlockError> {
    let live = closure(store, roots).await?;
    let dead = all_cids.difference(&live).map(Clone::clone).collect();
    Ok(dead)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cid::Cid;
    use crate::store::{BufStore, MemStore, StoreCborExt};
    use crate::{ipld, DefaultHash as H};
    use async_std::task;

    #[test]
    fn test_references() {
        let cid1 = Cid::new_v0(multihash::Sha2_256::digest(b"cid1")).unwrap();
        let cid2 = Cid::new_v0(multihash::Sha2_256::digest(b"cid2")).unwrap();
        let cid3 = Cid::new_v0(multihash::Sha2_256::digest(b"cid3")).unwrap();
        let ipld = ipld!({
            "cid1": &cid1,
            "cid2": { "other": true, "cid2": { "cid2": &cid2 }},
            "cid3": [[ &cid3, &cid1 ]],
        });
        let refs = references(&ipld);
        assert_eq!(refs.len(), 3);
        assert!(refs.contains(&cid1));
        assert!(refs.contains(&cid2));
        assert!(refs.contains(&cid3));
    }

    async fn run_test_closure() -> Result<(), BlockError> {
        let store = BufStore::new(MemStore::default(), 16, 16);
        let cid1 = store.write_cbor::<H, _>(&ipld!(true)).await?;
        let cid2 = store.write_cbor::<H, _>(&ipld!({ "cid1": &cid1 })).await?;
        let cid3 = store.write_cbor::<H, _>(&ipld!([&cid2])).await?;
        let mut roots: CidHashSet = Default::default();
        roots.insert(cid3.clone());
        let refs = closure(&store, roots).await?;
        assert_eq!(refs.len(), 3);
        assert!(refs.contains(&cid1));
        assert!(refs.contains(&cid2));
        assert!(refs.contains(&cid3));
        Ok(())
    }

    #[test]
    fn test_closure() {
        task::block_on(run_test_closure()).unwrap();
    }
}
