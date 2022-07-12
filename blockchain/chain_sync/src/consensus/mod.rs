use std::fmt::{Debug, Display};

// TODO: Move into its own crate.
mod filecoin;

pub trait Consensus: Debug + Send + 'static {
    type Error: Debug + Display + Send;
}
