//! Cœur d'Odasio : table de correspondance des blocs et pipeline de conversion.

pub mod block_entity;
pub mod catalog;
pub mod convert;
pub mod inventory;
pub mod mapping;
pub mod paths;
pub mod plan;

pub use convert::{
    ConvertOptions, ConvertReport, GameMode, Lighting, Progress, Season, WorldType, convert,
    standard_layout, vs_sea_level,
};
pub use inventory::Inventory;
pub use mapping::{Action, Category, Mapping, Policy, Quality, Resolved};
pub use odasio_mc as mc;
pub use odasio_vs as vs;
pub use plan::ConversionPlan;

pub const APP_NAME: &str = "OdasiO";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
