//! Fetchly core: the platform-independent engine behind the Fetchly API client.
//!
//! Compiles for native targets and `wasm32-unknown-unknown`.

pub mod codegen;
pub mod cookies;
pub mod curl;
pub mod http;
pub mod model;
pub mod pipeline;
pub mod postman;
pub mod script;
pub mod url;
pub mod vars;

pub use model::*;
