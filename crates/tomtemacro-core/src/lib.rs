//! TomteMacro engine: input event model, injection, capture, recording,
//! playback, and the auto-clicker. GUI-free and headless-testable; the egui
//! frontend lives in `tomtemacro-gui`.

pub mod capture;
pub mod clicker;
pub mod convert;
pub mod engine;
pub mod inject;
pub mod model;
pub mod platform;
pub mod player;
pub mod recorder;
pub mod script;
pub mod storage;
pub mod timing;
