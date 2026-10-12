//! AluKeys core: platform-independent remapping for Apple aluminium keyboards.

pub mod action;
pub mod config;
pub mod emitter;
pub mod engine;
pub mod keys;
pub mod model;
pub mod presets;

pub use action::{Action, Chord, Rule, Trigger};
pub use config::{Config, Keymap};
pub use emitter::Out;
pub use engine::{Effect, Engine, Response};
pub use keys::{Key, OutMods, PhysMods};
