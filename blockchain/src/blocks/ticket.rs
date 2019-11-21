// A Ticket is a marker of a tick of the blockchain's clock.  It is the source
// of randomness for proofs of storage and leader election.  It is generated
// by the miner of a block using a VRF and a VDF.
#[derive(Clone)]
pub struct Ticket {
    pub vrfproof: VRFPi,
}

// VRFPi is the proof output from running a VRF
pub type VRFPi = Vec<u8>;
