//! Talks to Dahua based recorders (Intelbras NVRs included) over their HTTP API and RTSP.

pub mod clips;
pub mod dahua;
pub mod dav;
pub mod layout;
pub mod recorders;
#[cfg(target_os = "linux")]
mod secret_portal;
pub mod update;
