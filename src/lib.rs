//! Everything except the window: scanning, launching and grid
//! sizing. Kept separate from the UI so it can be tested without compiling
//! the UI's (very deep) element types in test mode.

pub mod install;
pub mod launch;
pub mod layout;
pub mod scan;
