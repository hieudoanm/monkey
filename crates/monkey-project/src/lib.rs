//! Connect IQ project discovery: `monkey.jungle` and `manifest.xml` parsing.
//!
//! Zero dependencies. Used by `monkey doctor`, `monkey project`, and (later)
//! build/test orchestration.

pub mod jungle;
pub mod manifest;
pub mod project;
pub mod xml;

pub use jungle::Jungle;
pub use manifest::{Application, Manifest};
pub use project::{
    check_files, discover, discover_from, is_project_root, load, Project, MAX_DEPTH,
};
