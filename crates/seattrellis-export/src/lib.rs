//! SeatTrellis export/render primitives (M1-02).
//!
//! Turns a solved plan into SVG / HTML / PNG / PDF / XLSX / DOCX / PPTX bytes.
//! Split out of the app crate so transport/application never grow rendering
//! code; the CLI's own renderer mirrors these functions.

pub mod export;
pub mod office;
pub mod render;
