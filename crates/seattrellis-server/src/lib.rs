//! SeatTrellis loopback HTTP transport (M1-02).
//!
//! Routes, DTO formatting, the axum adapter ([`http`]) and the embedded
//! workbench assets ([`embedded_web`]). This is the only layer allowed to
//! touch `Request`/`Response`; business orchestration lives in
//! `seattrellis-application`.

pub mod http;
pub mod server;

mod embedded_web;
