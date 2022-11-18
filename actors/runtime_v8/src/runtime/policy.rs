pub use fil_actors_runtime_v9::runtime::Policy;

// A trait for runtime policy configuration
pub trait RuntimePolicy {
    fn policy(&self) -> &Policy;
}
