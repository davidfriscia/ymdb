//! Local artwork and screenshots ("media") for catalog records.
//!
//! - [`config`]: media categories, the configured source directories and
//!   `media.json` persistence.
//! - [`resolve`]: finds the image of a machine or software in the sources.
//! - [`protocol`]: the `ymdb-media` URI scheme serving resolved images to the
//!   webview from identifiers only, never from paths sent by the frontend.
//!
//! YMDB only reads the configured directories: it never copies, modifies or
//! indexes the images.

pub mod config;
pub mod protocol;
pub mod resolve;

#[cfg(test)]
mod tests;
