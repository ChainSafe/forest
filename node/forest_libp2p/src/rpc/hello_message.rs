use forest_cid::Cid;

/// Hello message https://filecoin-project.github.io/specs/#hello-spec
pub struct HelloMessage {
    heaviest_tip_set: Vec<Cid>,
    heaviest_tipset_weight: u64,
    heaviest_tipset_height: u64,
    genesis_hash: Cid,
}

/// Response to a Hello
pub struct LatencyMessage {
    /// Time of arrival in unix nanoseconds
    arrival: u64,
    /// Time sent in unix nanoseconds
    sent: u64,
}
