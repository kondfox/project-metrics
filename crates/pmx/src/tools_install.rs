//! `pmx tools install` (plan §2.2): the pinned Go tools (scc, osv-scanner, gitleaks), downloaded from
//! their GitHub releases and checked against SHA-256 digests compiled into pmx. semgrep (Python)
//! and jscpd (Node, via npx) are not installed this way; `pmx doctor` explains how.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use pm_snapshot::ToolId;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Packaging {
    /// The asset is the binary itself.
    Raw,
    TarGz,
    Zip,
}

#[derive(Clone, Copy, Debug)]
struct Asset {
    os: &'static str,
    arch: &'static str,
    name: &'static str,
    sha256: &'static str,
}

struct Release {
    id: ToolId,
    repo: &'static str,
    tag: &'static str,
    /// The binary's name inside the asset (without `.exe`).
    binary: &'static str,
    packaging: fn(&str) -> Packaging,
    assets: &'static [Asset],
}

const fn a(os: &'static str, arch: &'static str, name: &'static str, sha256: &'static str) -> Asset {
    Asset { os, arch, name, sha256 }
}

fn archive(name: &str) -> Packaging {
    if name.ends_with(".zip") {
        Packaging::Zip
    } else if name.ends_with(".tar.gz") {
        Packaging::TarGz
    } else {
        Packaging::Raw
    }
}

/// Pinned releases (versions as in `ToolId::pinned`). Digests from the GitHub release assets.
const RELEASES: &[Release] = &[
    Release {
        id: ToolId::Scc,
        repo: "boyter/scc",
        tag: "v4.1.0",
        binary: "scc",
        packaging: archive,
        assets: &[
            a(
                "macos",
                "aarch64",
                "scc_Darwin_arm64.tar.gz",
                "7201c7aa4aace058d43308462cba72adb18f6094c08c0741727455976d0a0747",
            ),
            a(
                "macos",
                "x86_64",
                "scc_Darwin_x86_64.tar.gz",
                "7f705031228add7e55edded409179a60de6b538d41f153ba2922dee95adda50d",
            ),
            a(
                "linux",
                "aarch64",
                "scc_Linux_arm64.tar.gz",
                "6e0d2a1f8d3540ba7df185477dec40bb7340f1b214bfd303147de5cad2bd7b8b",
            ),
            a(
                "linux",
                "x86_64",
                "scc_Linux_x86_64.tar.gz",
                "c7328436d3027f4357d3d7853f7dc3ac2bbcb4ca08f1adad91a27c593884079b",
            ),
            a(
                "windows",
                "aarch64",
                "scc_Windows_arm64.zip",
                "fdc1188bec07ab46fd391686c404abcd0bed92fffb5f1bb03aa9050dd7bc7f15",
            ),
            a(
                "windows",
                "x86_64",
                "scc_Windows_x86_64.zip",
                "4a433984f45ff29c94eeb3af6db5b511c58f5bf74063dccedafcbf473cb8ff31",
            ),
        ],
    },
    Release {
        id: ToolId::OsvScanner,
        repo: "google/osv-scanner",
        tag: "v2.5.1",
        binary: "osv-scanner",
        packaging: archive,
        assets: &[
            a(
                "macos",
                "aarch64",
                "osv-scanner_darwin_arm64",
                "75c44d6332f892a1e56286f4105a98ed751ae28d215ca0a8b65cc00d84103054",
            ),
            a(
                "macos",
                "x86_64",
                "osv-scanner_darwin_amd64",
                "9f89beb6c3d784893cb1cae0a3d56c529bfe91075418c2f9440c45b79654198b",
            ),
            a(
                "linux",
                "aarch64",
                "osv-scanner_linux_arm64",
                "3d0f5aa5a6baa8eb32bcef247388e149ef6030a6634ccae6fa0d62681fb27a6d",
            ),
            a(
                "linux",
                "x86_64",
                "osv-scanner_linux_amd64",
                "f9f25499a2c8cc367b3af45df2ea7eeca7fbccceab9c35079968f4b3652194be",
            ),
            a(
                "windows",
                "aarch64",
                "osv-scanner_windows_arm64.exe",
                "33feb0b210a3e5ea7b338c719defc899f8833d990cdd297bcad4ff1a2586ec8b",
            ),
            a(
                "windows",
                "x86_64",
                "osv-scanner_windows_amd64.exe",
                "25e42f5ef6711fd8c0fb45390972205891dd44c6bd02ac93f0f63e8e98d9bfb6",
            ),
        ],
    },
    Release {
        id: ToolId::Gitleaks,
        repo: "gitleaks/gitleaks",
        tag: "v8.30.1",
        binary: "gitleaks",
        packaging: archive,
        assets: &[
            a(
                "macos",
                "aarch64",
                "gitleaks_8.30.1_darwin_arm64.tar.gz",
                "b40ab0ae55c505963e365f271a8d3846efbc170aa17f2607f13df610a9aeb6a5",
            ),
            a(
                "macos",
                "x86_64",
                "gitleaks_8.30.1_darwin_x64.tar.gz",
                "dfe101a4db2255fc85120ac7f3d25e4342c3c20cf749f2c20a18081af1952709",
            ),
            a(
                "linux",
                "aarch64",
                "gitleaks_8.30.1_linux_arm64.tar.gz",
                "e4a487ee7ccd7d3a7f7ec08657610aa3606637dab924210b3aee62570fb4b080",
            ),
            a(
                "linux",
                "x86_64",
                "gitleaks_8.30.1_linux_x64.tar.gz",
                "551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb",
            ),
            a(
                "windows",
                "aarch64",
                "gitleaks_8.30.1_windows_arm64.zip",
                "b95f5e4f5c425cedca7ee203d9afd29597e692c4924a12ed42f970537c72cc0f",
            ),
            a(
                "windows",
                "x86_64",
                "gitleaks_8.30.1_windows_x64.zip",
                "d29144deff3a68aa93ced33dddf84b7fdc26070add4aa0f4513094c8332afc4e",
            ),
        ],
    },
];

fn asset_for(r: &Release, os: &str, arch: &str) -> Option<Asset> {
    r.assets.iter().find(|a| a.os == os && a.arch == arch).copied()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn verify(bytes: &[u8], expected: &str) -> Result<()> {
    let got = hex(&Sha256::digest(bytes));
    if got != expected {
        bail!("checksum mismatch: expected {expected}, got {got} (download refused)");
    }
    Ok(())
}

/// The binary out of a release asset.
fn extract(bytes: &[u8], packaging: Packaging, binary: &str) -> Result<Vec<u8>> {
    let exe = if cfg!(windows) {
        format!("{binary}.exe")
    } else {
        binary.to_string()
    };
    let wanted = |path: &Path| path.file_name().is_some_and(|n| n == exe.as_str());
    match packaging {
        Packaging::Raw => Ok(bytes.to_vec()),
        Packaging::TarGz => {
            let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
            for entry in ar.entries()? {
                let mut e = entry?;
                if wanted(&e.path()?) {
                    let mut out = Vec::new();
                    e.read_to_end(&mut out)?;
                    return Ok(out);
                }
            }
            Err(anyhow!("{exe} not found in the archive"))
        }
        Packaging::Zip => {
            let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
            for i in 0..z.len() {
                let mut f = z.by_index(i)?;
                if f.enclosed_name().is_some_and(|p| wanted(&p)) {
                    let mut out = Vec::new();
                    f.read_to_end(&mut out)?;
                    return Ok(out);
                }
            }
            Err(anyhow!("{exe} not found in the archive"))
        }
    }
}

fn download(url: &str) -> Result<Vec<u8>> {
    let mut res = ureq::get(url).call().with_context(|| format!("downloading {url}"))?;
    Ok(res.body_mut().with_config().limit(300 * 1024 * 1024).read_to_vec()?)
}

/// Install the pinned scc, osv-scanner and gitleaks into `bin`. Returns what was done, per tool.
pub fn install(bin: &Path, force: bool) -> Result<Vec<String>> {
    std::fs::create_dir_all(bin)?;
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let mut done = Vec::new();
    for r in RELEASES {
        let exe = if cfg!(windows) {
            format!("{}.exe", r.binary)
        } else {
            r.binary.to_string()
        };
        let dest: PathBuf = bin.join(&exe);
        if dest.exists() && !force {
            done.push(format!(
                "{} {}: already installed ({})",
                r.id.name(),
                r.id.pinned(),
                dest.display()
            ));
            continue;
        }
        let asset = asset_for(r, os, arch).ok_or_else(|| anyhow!("{}: no release for {os}/{arch}", r.id.name()))?;
        let url = format!(
            "https://github.com/{}/releases/download/{}/{}",
            r.repo, r.tag, asset.name
        );
        let bytes = download(&url)?;
        verify(&bytes, asset.sha256).with_context(|| format!("{}: {url}", r.id.name()))?;
        let binary = extract(&bytes, (r.packaging)(asset.name), r.binary)?;
        let tmp = bin.join(format!(".{exe}.partial"));
        std::fs::write(&tmp, binary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&tmp, &dest)?;
        done.push(format!(
            "{} {}: installed ({})",
            r.id.name(),
            r.id.pinned(),
            dest.display()
        ));
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_release_covers_the_main_platforms() {
        for r in RELEASES {
            for (os, arch) in [
                ("macos", "aarch64"),
                ("macos", "x86_64"),
                ("linux", "x86_64"),
                ("linux", "aarch64"),
                ("windows", "x86_64"),
            ] {
                let a = asset_for(r, os, arch).unwrap_or_else(|| panic!("{} {os}/{arch}", r.id.name()));
                assert_eq!(a.sha256.len(), 64);
            }
            assert_eq!(
                r.tag.trim_start_matches('v'),
                r.id.pinned(),
                "{} tag vs pinned version",
                r.id.name()
            );
        }
    }

    #[test]
    fn checksum_and_extraction() {
        // A tar.gz with the binary in a sub-folder, as release archives often have.
        let mut tar_bytes = Vec::new();
        {
            let gz = flate2::write::GzEncoder::new(&mut tar_bytes, flate2::Compression::fast());
            let mut b = tar::Builder::new(gz);
            let body = b"#!/bin/sh\necho fake\n";
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o755);
            h.set_cksum();
            let name = if cfg!(windows) { "pkg/scc.exe" } else { "pkg/scc" };
            b.append_data(&mut h, name, &body[..]).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        let digest = hex(&Sha256::digest(&tar_bytes));
        verify(&tar_bytes, &digest).unwrap();
        assert!(verify(&tar_bytes, &"0".repeat(64)).is_err());
        assert_eq!(
            extract(&tar_bytes, Packaging::TarGz, "scc").unwrap(),
            b"#!/bin/sh\necho fake\n"
        );
        assert!(extract(&tar_bytes, Packaging::TarGz, "gitleaks").is_err());
        assert_eq!(archive("x.zip"), Packaging::Zip);
        assert_eq!(archive("osv-scanner_linux_amd64"), Packaging::Raw);
    }
}
