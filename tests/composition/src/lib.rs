//! Support for the real-composition gate: locating a `jinnd` checkout,
//! building the PINNED daemon, building the seam kits, driving a live
//! daemon process, and speaking to the operator API. The tests live in
//! `tests/`.

pub mod api;
pub mod daemon;
pub mod kit;
