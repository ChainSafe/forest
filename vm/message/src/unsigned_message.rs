use actor::actor::{MethodNum, MethodParams};
use actor::actor_state::{CallSeqNum, TokenAmount};

//Should probably be a bignum
pub type GasAmount = u64;

pub type GasPrice = TokenAmount;

pub type Address = String;

pub struct UnsignedMessage {
    from: Address, //addr.Address
    to: Address,   // addr.address
    method: MethodNum,
    params: MethodParams,
    call_seq_num: CallSeqNum,
    value: TokenAmount,
    gas_price: GasPrice,
    gas_limit: GasAmount,
}
