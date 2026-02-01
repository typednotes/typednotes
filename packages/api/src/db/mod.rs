//! Database module for connection pool management.

#[cfg(feature = "server")]
mod pool;

#[cfg(feature = "server")]
pub use pool::get_pool;
