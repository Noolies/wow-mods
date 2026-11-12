//! Game adapters. World of Warcraft (with WoW Forever as the headline client) is
//! the first; each further game gets its own module with detection, mod scanning
//! and launching, and an id stored on `Installation::game`.

pub mod wow;

pub const WOW: &str = "wow";
