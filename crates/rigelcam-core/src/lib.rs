//! Talks to Dahua based recorders (Intelbras NVRs included) over their HTTP API and RTSP.

pub mod clips;
pub mod dahua;
pub mod dav;
pub mod layout;
pub mod recorders;
#[cfg(target_os = "linux")]
mod secret_portal;
pub mod update;

/// Hours, minutes and seconds of a number of seconds after midnight.
pub(crate) fn clock_parts(seconds: u32) -> (u32, u32, u32) {
    (seconds / 3600, seconds / 60 % 60, seconds % 60)
}
