//! Looks for a new version and installs it, on Windows, where nothing else updates the app.

use std::io::{Read, Write};
use std::process::Command;
use std::time::Duration;

use rigelcam_core::update::{is_newer, parse_manifest, verify, Manifest};
use ureq::tls::{RootCerts, TlsConfig};

const LATEST: &str = "https://github.com/eduardoghi/rigelcam/releases/latest/download/latest.json";
/// Pairs with the key that signs the releases, which never reaches this repository.
const PUBLIC_KEY: &str = "RWQVpwc+M7n+jxTZC6s3o/HGigZ8QwwoBKioG0ArPis9ddHTnV9dWTxO";
/// Far above an installer, so a bad answer can't fill the memory.
const MAX_INSTALLER_BYTES: u64 = 256 * 1024 * 1024;
const INSTALLER_PREFIX: &str = "rigelcam-update-";

fn agent() -> ureq::Agent {
    // The certificates of the system, since a company proxy can sign HTTPS with its own.
    let tls = TlsConfig::builder()
        .root_certs(RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .https_only(true)
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .into()
}

fn latest() -> Result<Manifest, String> {
    let text = agent()
        .get(LATEST)
        .call()
        .and_then(|mut response| response.body_mut().read_to_string())
        .map_err(|error| error.to_string())?;
    parse_manifest(&text).ok_or_else(|| "the manifest is not valid".to_owned())
}

/// The latest release, if it is newer than this app. No update isn't worth a message, since a
/// computer without internet, or behind a proxy it doesn't trust, is normal here.
pub fn check() -> Option<Manifest> {
    latest()
        .inspect_err(|error| eprintln!("could not check for updates ({error})"))
        .ok()
        .filter(|manifest| is_newer(&manifest.version, env!("CARGO_PKG_VERSION")))
}

/// Downloads an installer and returns it only if its signature holds.
fn download(manifest: &Manifest, progress: &mut dyn FnMut(f64)) -> Result<Vec<u8>, String> {
    let failed = |error: &dyn std::fmt::Display| error.to_string();
    let response = agent().get(&manifest.url).call().map_err(|e| failed(&e))?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok()?.parse::<f64>().ok());
    let mut reader = response.into_body().into_reader().take(MAX_INSTALLER_BYTES);
    let mut installer = Vec::new();
    let mut piece = vec![0; 64 * 1024];
    loop {
        let size = reader.read(&mut piece).map_err(|e| failed(&e))?;
        if size == 0 {
            break;
        }
        installer.extend_from_slice(&piece[..size]);
        if let Some(total) = total {
            progress((installer.len() as f64 / total).min(1.0));
        }
    }
    if !verify(&installer, manifest, PUBLIC_KEY) {
        return Err("the signature of the installer is not valid".to_owned());
    }
    Ok(installer)
}

/// Downloads the installer, checks it and starts it. Nothing runs unless the signature holds.
pub fn install(manifest: &Manifest, progress: &mut dyn FnMut(f64)) -> Result<(), String> {
    let failed = |error: &dyn std::fmt::Display| error.to_string();
    let installer = download(manifest, progress)?;
    let path = std::env::temp_dir().join(format!(
        "{INSTALLER_PREFIX}{}-{}.exe",
        manifest.version,
        std::process::id()
    ));
    let mut file = std::fs::File::create_new(&path).map_err(|e| failed(&e))?;
    file.write_all(&installer).map_err(|e| failed(&e))?;
    drop(file);
    Command::new(&path)
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/update=1"])
        .spawn()
        .map_err(|e| failed(&e))?;
    Ok(())
}

/// Removes installers of past updates, which can't delete themselves.
pub fn remove_old_installers() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(INSTALLER_PREFIX)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs in the release workflow, on a machine with a clean path to GitHub, since this reaches
    /// the published release through the same code the app uses.
    #[test]
    #[ignore = "needs the internet"]
    fn the_published_release_downloads_and_verifies() {
        let manifest = latest().unwrap();
        assert!(!download(&manifest, &mut |_| {}).unwrap().is_empty());
    }
}
