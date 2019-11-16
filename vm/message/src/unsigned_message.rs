use actor::{CallSeqNum, MethodNum, MethodParams, TokenAmount};

//Should probably be a bignum
pub type GasAmount = u64;

pub type GasPrice = TokenAmount;

pub type Address = String;

pub struct UnsignedMessage {
    pub from: Address, //addr.Address
    pub to: Address,   // addr.address
    pub method: MethodNum,
    pub params: MethodParams,
    pub call_seq_num: CallSeqNum,
    pub value: TokenAmount,
    pub gas_price: GasPrice,
    pub gas_limit: GasAmount,
}
