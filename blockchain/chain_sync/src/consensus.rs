use std::fmt::{Debug, Display};

pub trait Consensus: Debug + Send + 'static {
    type Error: Debug + Display + Send;
}
