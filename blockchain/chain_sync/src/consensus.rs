use std::fmt::{Debug, Display};

pub trait Consensus: Debug + Send + Sync + Unpin + 'static {
    type Error: Debug + Display + Send + Sync;
}
