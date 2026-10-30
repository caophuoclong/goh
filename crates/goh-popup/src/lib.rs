//! Presentational model of the suggestion popup (docs/07, design-system/components/SuggestionPopup).
//!
//! The adapter/daemon own the item list, the selected index and key handling; this crate only turns
//! that state into rows to draw (windowing, truncation, grapheme-safe match ranges, accessible
//! names) and decides whether an overlay may be drawn at all. No GUI types live here.

pub mod display;
pub mod font;
pub mod icons;
pub mod model;
pub mod placement;
pub mod watch;

pub mod platform;
