//! 資產清冊：資料模型、repository、Excel/CSV 匯入與匯出、畫面編修與套件清單匯入。

pub mod classes;
pub mod columns;
pub mod edit;
mod edit_lifecycle;
pub mod export;
pub mod import;
pub mod ips;
pub mod isms;
pub mod model;
pub mod packages;
pub mod pending;
pub mod plan;
mod plan_fields;
mod plan_identity;
mod plan_systems;
mod record;
mod record_admin;
pub mod record_identity;
pub mod repo;
pub mod sheet;
pub mod systems;
pub mod systems_edit;
pub mod template;
pub mod unique;
pub mod validate;
mod validate_ips;
mod validate_systems;

pub use classes::{AssetClass, AssetClasses, Cia, ClassCode, MajorClass};
pub use ips::{AssetIp, AssetIpRepo, IpError, IpErrorKind, IpInput};
pub use model::*;
pub use pending::{pending_counts, PendingCounts};
pub use repo::{AssetFilter, AssetRepo, ClassFilter, ComponentRepo, PackageUpsert, UpsertOutcome};
pub use systems::{AssetSystem, SystemCode, SystemDirectory};
