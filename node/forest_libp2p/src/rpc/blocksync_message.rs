use forest_blocks::BlockHeader;
use forest_encoding::{
    de::{self, Deserialize, Deserializer},
    ser::{self, Serialize, Serializer},
};
use forest_encoding::{to_vec, Cbor};
use forest_message::{SignedMessage, UnsignedMessage};
use forest_cid::Cid;

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub start: Vec<Cid>,
    pub request_len: u64,
    pub options: u64,
}

impl Serialize for Message {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
    {
        let value = (self.start.clone(), self.request_len, self.options);
        ser::Serialize::serialize(&value, serializer)
    }
}
impl<'de> Deserialize<'de> for Message {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
    {
        let (start, request_len, options) = Deserialize::deserialize(deserializer)?;
        Ok(Message {
            start,
            request_len,
            options,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Response {
    pub chain: Vec<TipSetBundle>,
    pub status: u64,
    pub message: String,
}

impl Serialize for Response {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
        where
            S: Serializer,
    {
        let value = (self.chain.clone(), self.status, self.message.clone());
        Serialize::serialize(&value, serializer)
    }
}
impl<'de> Deserialize<'de> for Response {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
        where
            D: Deserializer<'de>,
    {
        let (chain, status, message) = Deserialize::deserialize(deserializer)?;
        Ok(Response {
            chain,
            status,
            message,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TipSetBundle {
    pub blocks: Vec<BlockHeader>,
    pub secp_msgs: Vec<UnsignedMessage>,
    pub secp_msg_includes: Vec<Vec<u64>>,

    pub bls_msgs: Vec<SignedMessage>,
    pub bls_msg_includes: Vec<Vec<u64>>,
}

impl ser::Serialize for TipSetBundle {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
        where
            S: Serializer,
    {
        let value = (
            self.blocks.clone(),
            self.secp_msgs.clone(),
            self.secp_msg_includes.clone(),
            self.bls_msgs.clone(),
            self.bls_msg_includes.clone(),
        );
        Serialize::serialize(&value, serializer)
    }
}

impl<'de> de::Deserialize<'de> for TipSetBundle {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
        where
            D: Deserializer<'de>,
    {
        let (blocks, secp_msgs, secp_msg_includes, bls_msgs, bls_msg_includes) =
            Deserialize::deserialize(deserializer)?;
        Ok(TipSetBundle {
            blocks,
            secp_msgs,
            secp_msg_includes,
            bls_msgs,
            bls_msg_includes,
        })
    }
}
