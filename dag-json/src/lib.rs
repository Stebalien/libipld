//! Json codec.
use libipld_core::codec::{Code, Codec, Decode, Encode};
use libipld_core::ipld::Ipld;
use serde_json::Error;
use std::io::{Read, Write};

mod codec;

/// Json codec.
#[derive(Clone, Copy, Debug)]
pub struct DagJson;

impl Codec for DagJson {
    const CODE: Code = Code::DagJSON;

    type Error = Error;
}

impl Encode<DagJson> for Ipld {
    fn encode<W: Write>(&self, w: &mut W) -> Result<(), Error> {
        codec::encode(self, w)
    }
}

impl Decode<DagJson> for Ipld {
    fn decode<R: Read>(r: &mut R) -> Result<Self, Error> {
        codec::decode(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libipld_core::cid::Cid;
    use libipld_core::multihash::Sha2_256;
    use std::collections::BTreeMap;

    #[test]
    fn encode_struct() {
        let digest = Sha2_256::digest(b"block");
        let cid = Cid::new_v0(digest).unwrap();

        // Create a contact object that looks like:
        // Contact { name: "Hello World", details: CID }
        let mut map = BTreeMap::new();
        map.insert("name".to_string(), Ipld::String("Hello World!".to_string()));
        map.insert("details".to_string(), Ipld::Link(cid.clone()));
        let contact = Ipld::Map(map);

        let contact_encoded = DagJson::encode(&contact).unwrap();
        println!("encoded: {:02x?}", contact_encoded);
        println!(
            "encoded string {}",
            std::str::from_utf8(&contact_encoded).unwrap()
        );

        assert_eq!(
            std::str::from_utf8(&contact_encoded).unwrap(),
            format!(
                r#"{{"details":{{"/":"{}"}},"name":"Hello World!"}}"#,
                base64::encode(cid.to_bytes()),
            )
        );

        let contact_decoded: Ipld = DagJson::decode(&contact_encoded).unwrap();
        assert_eq!(contact_decoded, contact);
    }
}
