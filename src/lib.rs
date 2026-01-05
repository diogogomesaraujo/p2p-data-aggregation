use std::time::Duration;

pub mod log;
pub mod peer;
pub mod poisson;

pub const RATE: f32 = 2.;
pub const WAIT_TIME: Duration = Duration::from_secs(1);
