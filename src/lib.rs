pub mod error;
pub use error::Error;

#[macro_use]
extern crate lazy_static;

#[cfg(all(windows, feature = "edgehtml"))]
pub mod edge;
pub mod edge_winit;
