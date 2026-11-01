//! Decides whether an update exists and whether its installer can be trusted.

use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;

/// Where installers come from. The signature is what protects them, and this keeps a manifest from
/// sending the app anywhere else to begin with.
const DOWNLOADS: &str = "https://github.com/eduardoghi/rigelcam/releases/download/";

/// What the latest release offers.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub version: String,
    pub url: String,
    /// The whole `.minisig` file of the installer.
    pub signature: String,
}

/// Reads a `latest.json`, refusing one that points outside the releases of the project.
pub fn parse_manifest(text: &str) -> Option<Manifest> {
    let manifest: Manifest = serde_json::from_str(text).ok()?;
    manifest.url.starts_with(DOWNLOADS).then_some(manifest)
}

fn numbers(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.').map(|part| part.parse::<u32>().ok());
    let numbers = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(numbers)
}

/// Whether `candidate` comes after `current`, both as `major.minor.patch`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((numbers(candidate), numbers(current)), (Some(new), Some(old)) if new > old)
}

/// Checks an installer against the key built into the app. The version is in the signed comment,
/// because a signature on the file alone would let a release offer an older signed installer
/// under a higher version number.
pub fn verify(installer: &[u8], manifest: &Manifest, public_key: &str) -> bool {
    let Ok(key) = PublicKey::from_base64(public_key) else {
        return false;
    };
    let Ok(signature) = Signature::decode(&manifest.signature) else {
        return false;
    };
    key.verify(installer, &signature, false).is_ok()
        && signature.trusted_comment() == format!("rigelcam {}", manifest.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A key made for these tests, which signed INSTALLER with the comment `rigelcam 1.2.3`.
    const KEY: &str = "RWR4U9E4PYJ8Ny4x6JNJ1vYBb+W7DNn3wEmzJfNgMsJJ/VGA01K4pw7j";
    const INSTALLER: &[u8] = b"instalador de teste";
    const SIGNATURE: &str = "untrusted comment: signature from minisign secret key\n\
         RUR4U9E4PYJ8N5av6Z0ym4be1x9q/FXq8JOUv/gWgZ5+zJrubY/VCLyWA/wnR70vE3vMxg/0mD0R5aG6ClbsCaUeOKu5QOeY8wA=\n\
         trusted comment: rigelcam 1.2.3\n\
         Eifp6pxboFEPaSo+zAnDplK7vxOp9+FicB5X1DWnW8jchHc05o0PjDw+QeEnX3FAextGF9zvOGFhAY1W/lVnDA==\n";

    fn manifest(version: &str) -> Manifest {
        Manifest {
            version: version.to_owned(),
            url: format!("{DOWNLOADS}v{version}/setup.exe"),
            signature: SIGNATURE.to_owned(),
        }
    }

    #[test]
    fn accepts_a_signed_installer_of_the_version_offered() {
        assert!(verify(INSTALLER, &manifest("1.2.3"), KEY));
    }

    #[test]
    fn refuses_another_file() {
        assert!(!verify(b"outro instalador", &manifest("1.2.3"), KEY));
    }

    #[test]
    fn refuses_an_old_installer_offered_as_a_newer_version() {
        assert!(!verify(INSTALLER, &manifest("1.2.4"), KEY));
    }

    #[test]
    fn refuses_a_signature_from_another_key() {
        let other = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        assert!(!verify(INSTALLER, &manifest("1.2.3"), other));
    }

    #[test]
    fn reads_a_manifest_only_from_the_releases_of_the_project() {
        let text = |url: &str| {
            serde_json::json!({"version": "1.2.3", "url": url, "signature": "s"}).to_string()
        };
        let own = format!("{DOWNLOADS}v1.2.3/setup.exe");
        assert_eq!(parse_manifest(&text(&own)).unwrap().url, own);
        assert!(parse_manifest(&text("https://example.com/setup.exe")).is_none());
        assert!(parse_manifest("not json").is_none());
    }

    #[test]
    fn compares_versions_by_number() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("0.2", "0.1.0"));
        assert!(!is_newer("0.2.0-beta", "0.1.0"));
    }
}
