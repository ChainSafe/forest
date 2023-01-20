// use fvm_shared::version::NetworkVersion as NetworkVersion_v2;
use fvm_shared3::version::NetworkVersion as NetworkVersion_v3;

pub type NetworkVersion = NetworkVersion_v3;

// XXX: Do we ever need to convert from NetworkVersion_v3 to NetworkVersion_v2?
