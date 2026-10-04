//! lyco-mcp - an MCP server exposing the lyco asset toolkit (F9).
//!
//! Start it and speak newline-delimited JSON-RPC 2.0 over stdin/stdout:
//!
//!     lyco-cli mcp
//!
//! The original F9 note scoped this as a code-generation server. Exposing the
//! whole toolkit turned out to be the more useful reading: an agent driving
//! this project wants to *produce assets*, and every one of those capabilities
//! already existed as a crate. Code generation stays available as an ordinary
//! backend-plumbed concern rather than a separate server.
//!
//! Modules:
//!
//!   * [`jsonrpc`] - the protocol slice: success, error and tool-result shapes
//!   * [`tools`] - the twelve tools and their JSON Schemas
//!   * [`server`] - dispatch and the stdio transport

pub mod jsonrpc;
pub mod server;
pub mod tools;

pub use jsonrpc::PROTOCOL_VERSION;
pub use server::{handle_line, serve_stdio, SERVER_NAME};
pub use tools::{Registry, Tool};
