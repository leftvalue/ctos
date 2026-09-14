//! `ctos update` — self-update from GitHub Releases.
//!
//! Lean stack on purpose (no reqwest/tokio): `ureq` for HTTPS, `flate2` +
//! `tar` / `zip` to unpack, `sha2` to verify the asset digest reported by the
//! GitHub API, and `self-replace` for the one genuinely hard part — atomically
//! replacing the running binary (including the Windows rename dance).
//!
//! Policy:
//!   * interactive confirmation when stdin is a TTY (auto-yes when piped),
//!     `--yes` skips the prompt;
//!   * `--check` only reports, never downloads;
//!   * Docker installs refuse (pull a new image instead);
//!   * cargo-installed binaries warn (canonical path: cargo install) but update.

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

const RELEASES_LATEST_URL: &str = "https://api.github.com/repos/leftvalue/ctos/releases/latest";
/// Marker file baked into the Docker image; `update` refuses when present.
const DOCKER_MARKER: &str = "/.ctos-docker";

/// The release asset for the current platform, or an error message.
pub fn asset_name() -> Result<&'static str> {
    let name = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "ctos-x86_64-unknown-linux-musl.tar.gz",
        ("linux", "aarch64") => "ctos-aarch64-unknown-linux-musl.tar.gz",
        ("macos", "x86_64") => "ctos-x86_64-apple-darwin.tar.gz",
        ("macos", "aarch64") => "ctos-aarch64-apple-darwin.tar.gz",
        ("windows", "x86_64") => "ctos-x86_64-pc-windows-msvc.zip",
        (os, arch) => bail!("no prebuilt ctos binary for {os}-{arch}"),
    };
    Ok(name)
}

/// Parse a `vX.Y.Z` (or bare `X.Y.Z`) tag into a comparable tuple.
pub fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let v = tag.strip_prefix('v').unwrap_or(tag);
    let mut it = v.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next()?.parse().ok()?;
    let patch = it.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

fn current_version() -> (u64, u64, u64) {
    parse_version(env!("CARGO_PKG_VERSION")).expect("crate version parses")
}

/// Whether the running binary lives in `~/.cargo/bin` (cargo install).
fn is_cargo_install(exe: &Path) -> bool {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
            Some(PathBuf::from(home).join(".cargo"))
        });
    let Some(cargo_bin) = cargo_home.map(|h| h.join("bin")) else {
        return false;
    };
    exe.parent() == Some(cargo_bin.as_path())
}

#[derive(Debug)]
struct ReleaseInfo {
    tag: String,
    url: String,
    /// sha256 hex digest reported by the GitHub API, when available.
    digest: Option<String>,
}

fn fetch_latest(agent: &ureq::Agent, api_url: &str) -> Result<ReleaseInfo> {
    let resp = agent.get(api_url).call().map_err(|e| {
        if let ureq::Error::Status(code, _) = &e {
            anyhow::anyhow!(
                "GitHub returned HTTP {code} for the release query — likely the \
                 unauthenticated rate limit (60/h per IP); try again later or check \
                 https://github.com/leftvalue/ctos/releases directly"
            )
        } else {
            anyhow::anyhow!("failed to reach GitHub for the latest release (offline?): {e}")
        }
    })?;
    let v: serde_json::Value = resp
        .into_json()
        .context("failed to parse the release response")?;

    let tag = v["tag_name"]
        .as_str()
        .context("release response has no tag_name")?
        .to_string();

    let want = asset_name()?;
    let asset = v["assets"]
        .as_array()
        .context("release has no assets")?
        .iter()
        .find(|a| a["name"].as_str() == Some(want))
        .with_context(|| format!("release {tag} has no asset {want}"))?;

    let url = asset["browser_download_url"]
        .as_str()
        .context("asset has no download url")?
        .to_string();
    let digest = asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(|d| d.to_ascii_lowercase());

    Ok(ReleaseInfo { tag, url, digest })
}

fn download(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>> {
    let resp = agent
        .get(url)
        .call()
        .context("failed to download the release asset")?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take(512 * 1024 * 1024) // hard cap: 512 MiB
        .read_to_end(&mut buf)
        .context("download interrupted")?;
    Ok(buf)
}

fn verify_digest(data: &[u8], expected: Option<&str>) -> Result<()> {
    let Some(expected) = expected else {
        eprintln!(
            "[ctos] warning: release asset has no sha256 digest on GitHub; skipping verification"
        );
        return Ok(());
    };
    use sha2::{Digest, Sha256};
    let actual = Sha256::digest(data);
    let hex: String = actual.iter().map(|b| format!("{b:02x}")).collect();
    if hex != expected {
        bail!("sha256 mismatch: downloaded {hex}, release says {expected} — refusing to install");
    }
    Ok(())
}

/// Extract the single ctos binary from a tar.gz or zip archive.
fn extract_binary(archive: &[u8], asset: &str) -> Result<Vec<u8>> {
    let wanted = if cfg!(windows) { "ctos.exe" } else { "ctos" };

    if asset.ends_with(".tar.gz") {
        let gz = flate2::read::GzDecoder::new(archive);
        let mut tar = tar::Archive::new(gz);
        for entry in tar.entries().context("failed to read the tar archive")? {
            let mut entry = entry.context("corrupt tar entry")?;
            let name = entry
                .path()
                .context("tar entry has no path")?
                .to_string_lossy()
                .to_string();
            if name == wanted || name.ends_with(&format!("/{wanted}")) {
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .context("failed to read binary")?;
                return Ok(buf);
            }
        }
        bail!("no {wanted} entry in the tar archive");
    }

    let cursor = std::io::Cursor::new(archive);
    let mut zip = zip::ZipArchive::new(cursor).context("failed to read the zip archive")?;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .with_context(|| format!("corrupt zip entry {i}"))?;
        let name = entry.name().to_string();
        if name == wanted || name.ends_with(&format!("/{wanted}")) {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .context("failed to read binary")?;
            return Ok(buf);
        }
    }
    bail!("no {wanted} entry in the zip archive");
}

/// Replace the running binary with `new_bytes`.
fn self_replace(new_bytes: &[u8]) -> Result<()> {
    let exe = std::env::current_exe().context("cannot locate the running binary")?;
    let dir = exe
        .parent()
        .context("running binary has no parent directory")?;
    let ext = exe
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let tmp = dir.join(format!(".ctos-update-tmp-{ext}"));

    std::fs::write(&tmp, new_bytes).context("failed to write the new binary")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .context("failed to make the new binary executable")?;
    }

    self_replace::self_replace(&tmp).context("failed to replace the running binary")?;
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

/// Entry point. `check_only` = `--check`; `yes` skips the interactive prompt.
pub fn run(check_only: bool, yes: bool) -> Result<()> {
    let current = current_version();
    println!("ctos v{}", env!("CARGO_PKG_VERSION"));

    if Path::new(DOCKER_MARKER).exists() {
        bail!(
            "this ctos lives inside the Docker image — update with:\n  \
             docker pull ghcr.io/leftvalue/ctos:latest"
        );
    }

    let agent: ureq::Agent = ureq::AgentBuilder::new()
        .user_agent(concat!("ctos/", env!("CARGO_PKG_VERSION")))
        .build();
    let release = fetch_latest(&agent, RELEASES_LATEST_URL)?;
    let latest = parse_version(&release.tag)
        .with_context(|| format!("unparseable release tag {:?}", release.tag))?;

    if latest <= current {
        println!("already up to date (latest: {})", release.tag);
        return Ok(());
    }

    if check_only {
        println!(
            "update available: v{} → {} — run `ctos update` to install",
            env!("CARGO_PKG_VERSION"),
            release.tag
        );
        return Ok(());
    }

    // Interactive confirmation on a TTY; piped/scripted invocations proceed.
    if !yes && std::io::stdin().is_terminal() {
        eprint!(
            "update ctos v{} → {}? [y/N] ",
            env!("CARGO_PKG_VERSION"),
            release.tag
        );
        std::io::stderr().flush().ok();
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .context("failed to read confirmation")?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("aborted");
            return Ok(());
        }
    }

    if is_cargo_install(&std::env::current_exe()?) {
        eprintln!(
            "[ctos] note: this binary was installed via `cargo install`; the canonical \
             upgrade path is:\n  cargo install --git https://github.com/leftvalue/ctos \
             --tag {}",
            release.tag
        );
    }

    println!("downloading {} …", asset_name()?);
    let bytes = download(&agent, &release.url)?;
    verify_digest(&bytes, release.digest.as_deref())?;
    let binary = extract_binary(&bytes, asset_name()?)?;
    self_replace(&binary)?;
    println!(
        "updated ctos v{} → {} — `ctos --version` to verify",
        env!("CARGO_PKG_VERSION"),
        release.tag
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing_and_ordering() {
        assert_eq!(parse_version("v0.2.3"), Some((0, 2, 3)));
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("v1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("vX.Y.Z"), None);
        assert_eq!(parse_version(""), None);
        assert!((0, 2, 4) > (0, 2, 3));
        assert!((1, 0, 0) > (0, 9, 9));
        assert!((0, 2, 3) == parse_version("v0.2.3").unwrap());
    }

    #[test]
    fn asset_name_matches_current_platform() {
        // Must return a known asset for the platform the tests run on.
        let name = asset_name().expect("test platform has an asset");
        assert!(name.starts_with("ctos-"));
        assert!(name.ends_with(".tar.gz") || name.ends_with(".zip"));
    }

    #[test]
    fn cargo_install_detection() {
        // A path NOT under ~/.cargo/bin.
        assert!(!is_cargo_install(Path::new("/usr/local/bin/ctos")));
        assert!(!is_cargo_install(Path::new("/tmp/ctos")));
    }

    #[test]
    fn digest_verification() {
        use sha2::{Digest, Sha256};
        let data = b"hello ctos";
        let digest = Sha256::digest(data);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert!(verify_digest(data, Some(&hex)).is_ok());
        assert!(verify_digest(data, Some("deadbeef")).is_err());
        // None => warn-and-continue.
        assert!(verify_digest(data, None).is_ok());
    }

    #[test]
    fn extract_from_tar_gz() {
        // Build a small tar.gz in memory containing ./ctos
        let mut tarbuf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tarbuf);
            let mut header = tar::Header::new_gnu();
            let content: &[u8] = b"#!/bin/sh\necho fake-ctos\n";
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, "ctos", content).unwrap();
            builder.finish().unwrap();
        }
        let mut gz = Vec::new();
        {
            use std::io::Write as _;
            let mut enc = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::default());
            enc.write_all(&tarbuf).unwrap();
            enc.finish().unwrap();
        }
        let bin = extract_binary(&gz, "ctos-x86_64-unknown-linux-musl.tar.gz").unwrap();
        assert!(bin.starts_with(b"#!/bin/sh"));
    }

    #[test]
    fn fetch_latest_parses_mock_release() {
        // Serve a GitHub-shaped release payload on a local TCP port so the
        // whole query/parse/select path is exercised without GitHub (the
        // sandbox IP is often API-rate-limited).
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;

        let asset = asset_name().unwrap();
        let body = format!(
            r#"{{"tag_name":"v9.9.9","assets":[
                {{"name":"README.txt","browser_download_url":"https://example.com/README.txt"}},
                {{"name":"{asset}","browser_download_url":"https://example.com/{asset}","digest":"sha256:ABCDEF"}}
            ]}}"#
        );

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).unwrap(); // consume the request
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).unwrap();
        });

        let agent: ureq::Agent = ureq::AgentBuilder::new().user_agent("ctos-test").build();
        let info = fetch_latest(&agent, &format!("http://{addr}/repos/x/y/releases/latest"))
            .expect("mock fetch succeeds");
        handle.join().unwrap();

        assert_eq!(info.tag, "v9.9.9");
        assert_eq!(info.url, format!("https://example.com/{asset}"));
        assert_eq!(info.digest.as_deref(), Some("abcdef"));
    }

    #[test]
    fn fetch_latest_reports_http_status() {
        use std::io::Write as _;
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut buf).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let agent: ureq::Agent = ureq::AgentBuilder::new().user_agent("ctos-test").build();
        let err = fetch_latest(&agent, &format!("http://{addr}/")).unwrap_err();
        handle.join().unwrap();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("HTTP 403"),
            "message should mention the status: {msg}"
        );
        assert!(
            msg.contains("rate limit"),
            "message should hint the rate limit: {msg}"
        );
    }
}
