//! The recording player (`--player <file>`): plays a video, seeks and sets the volume.
//! It runs in its own process, like the history window, so its memory is released on close.

mod controls;
mod engine;
mod window;

pub use window::run;

/// ほかのテスト（モンキーテスト）が、仕上がった録画を再生できるか確かめるのに使う。
#[cfg(test)]
pub(crate) use engine::{Engine, Status};

#[cfg(test)]
mod tests;
