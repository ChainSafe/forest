use super::actor::{MethodNum, MethodParams, CallSeqNum, TokenAmount};

//Should probably be a bignum
pub type GasAmount = u64;

pub type GasPrice = TokenAmount;

pub type Address = String;

pub type FilCryptoSignature = String; 

#[derive (Debug, Clone)]
pub struct Message {
    from: Address, //addr.Address
    to: Address, // addr.address
    method: MethodNum,
    params: MethodParams,
    call_seq_num: CallSeqNum,
    value: TokenAmount, 
    gas_price: GasPrice,
    gas_limit: GasAmount,
}

#[derive (Debug, Clone)]
pub struct SignedMessage {
    message: Message,
    signature: FilCryptoSignature,
}

impl From<SignedMessage> for Message {
    fn from (msg: SignedMessage) -> Self{
        msg.message
    }
}

impl Message {
   fn sign () -> SignedMessage {
       unimplemented!();
   } 
}
