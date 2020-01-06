//! Block validation
use crate::cid::Cid;
use crate::error::BlockError;
use crate::hash::{digest, Hash};
use crate::ipld::Ipld;
use crate::MAX_BLOCK_SIZE;
use dag_cbor::{decode::Read, Codec, DagCborCodec, ReadCbor, WriteCbor};

/// Validate block.
pub fn validate(cid: &Cid, data: &[u8]) -> Result<(), BlockError> {
    if data.len() > MAX_BLOCK_SIZE {
        return Err(BlockError::BlockToLarge(data.len()));
    }
    let hash = digest(cid.hash().algorithm(), &data);
    if hash.as_ref() != cid.hash() {
        return Err(BlockError::InvalidHash(hash));
    }
    Ok(())
}

/// Create raw block.
pub fn create_raw_block<H: Hash>(data: Box<[u8]>) -> Result<(Cid, Box<[u8]>), BlockError> {
    if data.len() > MAX_BLOCK_SIZE {
        return Err(BlockError::BlockToLarge(data.len()));
    }
    let hash = H::digest(&data);
    let cid = Cid::new_v1(cid::Codec::Raw, hash);
    Ok((cid, data))
}

/// Create cbor block.
pub async fn create_cbor_block<H: Hash, C: WriteCbor>(
    c: &C,
) -> Result<(Cid, Box<[u8]>), BlockError> {
    let mut data = Vec::new();
    c.write_cbor(&mut data).await?;
    if data.len() > MAX_BLOCK_SIZE {
        return Err(BlockError::BlockToLarge(data.len()));
    }
    let hash = H::digest(&data);
    let cid = Cid::new_v1(DagCborCodec::CODEC, hash);
    Ok((cid, data.into_boxed_slice()))
}

/// Decode block to ipld.
pub async fn decode_ipld(cid: &Cid, data: &[u8]) -> Result<Ipld, BlockError> {
    let ipld = match cid.codec() {
        DagCborCodec::CODEC => DagCborCodec::decode(&data).await?,
        cid::Codec::Raw => Ipld::Bytes(data.to_vec()),
        _ => return Err(BlockError::UnsupportedCodec(cid.codec())),
    };
    Ok(ipld)
}

/// Decode block from cbor.
pub async fn decode_cbor<C: ReadCbor>(cid: &Cid, mut data: &[u8]) -> Result<C, BlockError> {
    decode_cbor_read(cid, &mut data).await
}

/// Decode block from a cbor reader.
pub async fn decode_cbor_read<R: Read + Unpin + Send, C: ReadCbor>(
    cid: &Cid,
    reader: &mut R,
) -> Result<C, BlockError> {
    if cid.codec() != DagCborCodec::CODEC {
        return Err(BlockError::UnsupportedCodec(cid.codec()));
    }
    let res = C::read_cbor(reader).await?;
    Ok(res)
}
