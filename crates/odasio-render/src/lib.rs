//! Prévisualisation isométrique des mondes : aspects des blocs lus dans les installations de
//! Minecraft et de Vintage Story, et rendu logiciel en image RGBA.

pub mod area;
pub mod iso;
pub mod look;
pub mod mc_assets;
pub mod vs_assets;

pub use look::{BlockLook, Face, Shape, Texture};
