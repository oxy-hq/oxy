// SeaORM 2.0's query types nest deeper generically than 1.1's. This crate
// has no sea-orm dependency of its own, but its async fns transitively await
// futures that hold them, and laying those out now exceeds rustc's default
// query depth. Raising the limit is the fix rustc itself suggests.
#![recursion_limit = "256"]

pub mod adapter;
pub mod config;
pub mod error;
pub mod service;
pub mod types;
