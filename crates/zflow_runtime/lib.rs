pub mod component;
mod component_test;
pub mod errors;
pub mod ip;

pub mod network;

pub mod port;

mod port_test;

pub mod process;

pub mod sockets;

pub mod network_manager;

pub mod provider;
pub mod runner;

#[cfg(feature = "host_only")]
pub mod extern_provider;


// #[cfg(feature = "js_runtime")]
// pub mod js;
// #[cfg(feature = "lua_runtime")]
// pub mod lua;

#[cfg(feature = "host_only")]
pub mod wasm;
#[cfg(feature = "host_only")]
pub mod deno;
#[cfg(feature = "host_only")]
pub mod websocket;
// #[cfg(feature = "wren_runtime")]
// pub mod wren;
// #[cfg(feature = "go_runtime")]
// pub mod go;

mod network_test;

// #[cfg(feature = "go_runtime")]
// #[macro_use]
// extern crate go_engine as goengine;
