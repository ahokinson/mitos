mod files;
mod installs;
mod locations;
mod parsers;
mod reports;

pub use installs::{init, status};
pub use locations::Locations;
pub use parsers::parse;
pub use reports::{HookStatus, InitOutcome, InitResult, Trust};
