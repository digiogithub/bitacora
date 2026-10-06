#![allow(clippy::expect_used, clippy::unwrap_used)]
//! `self-update` against a local fake release server (no network).

use std::collections::HashMap;
use std::io::Write as _;
use std::net::TcpListener;

use super::*;

const TARGET: &str = "t-t";

fn targz(content: &[u8]) -> Vec<u8> {
    let mut tar_bytes = Vec::new();
    {
        let mut b = tar::Builder::new(&mut tar_bytes);
        let mut h = tar::Header::new_gnu();
        h.set_size(content.len() as u64);
        h.set_mode(0o755);
        h.set_cksum();
        b.append_data(&mut h, "bitacora-cli-9.9.9-x/bitacora-cli", content)
            .expect("append");
        b.finish().expect("finish");
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&tar_bytes).expect("gz");
    gz.finish().expect("gz finish")
}

fn zipped(content: &[u8]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    w.start_file("bitacora-cli.exe", zip::write::SimpleFileOptions::default())
        .expect("start");
    w.write_all(content).expect("write");
    w.finish().expect("finish").into_inner()
}

struct Fixture {
    api: String,
    _dir: tempfile::TempDir,
    exe: PathBuf,
}

/// Fake GitHub: a releases listing plus the asset downloads, on an ephemeral port.
fn fixture(tag: &str, prerelease: bool, corrupt_digest: bool) -> Fixture {
    let archive = targz(b"NEW-BINARY");
    let name = format!(
        "bitacora-cli-{}-{TARGET}.tar.gz",
        tag.trim_start_matches('v')
    );
    let digest = if corrupt_digest {
        "0".repeat(64)
    } else {
        hex(&Sha256::digest(&archive))
    };
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let listing = serde_json::json!([
        { "tag_name": tag, "draft": false, "prerelease": prerelease, "assets": [
            { "name": name, "browser_download_url": format!("{base}/dl/{name}") },
            { "name": "SHA256SUMS", "browser_download_url": format!("{base}/dl/SHA256SUMS") },
        ]},
        { "tag_name": "v0.0.1", "draft": false, "prerelease": false, "assets": [] },
        { "tag_name": "v99.0.0", "draft": true, "prerelease": false, "assets": [] },
    ]);
    let mut routes: HashMap<String, Vec<u8>> = HashMap::new();
    routes.insert("/releases".into(), listing.to_string().into_bytes());
    routes.insert(format!("/dl/{name}"), archive);
    routes.insert(
        "/dl/SHA256SUMS".into(),
        format!("{digest}  {name}\n").into_bytes(),
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .split('?')
                .next()
                .unwrap_or("/")
                .to_string();
            let (status, body) = match routes.get(&path) {
                Some(b) => ("200 OK", b.clone()),
                None => ("404 Not Found", Vec::new()),
            };
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(&body);
        }
    });
    let dir = tempfile::tempdir().expect("tmp");
    let exe = dir.path().join("bitacora-cli");
    std::fs::write(&exe, b"OLD-BINARY").expect("write exe");
    Fixture {
        api: base,
        _dir: dir,
        exe,
    }
}

fn args(api: &str) -> SelfUpdateArgs {
    SelfUpdateArgs {
        check: false,
        prerelease: false,
        force: false,
        api_url: api.to_string(),
    }
}

#[test]
fn version_ordering() {
    use Ordering::*;
    assert_eq!(compare_versions("1.2.3", "1.2.3"), Some(Equal));
    assert_eq!(compare_versions("v1.10.0", "1.9.9"), Some(Greater));
    assert_eq!(compare_versions("1.0.0-beta.2", "1.0.0"), Some(Less));
    assert_eq!(
        compare_versions("1.0.0-beta.10", "1.0.0-beta.2"),
        Some(Greater)
    );
    assert_eq!(
        compare_versions("1.0.0-rc.1", "1.0.0-beta.9"),
        Some(Greater)
    );
    assert_eq!(compare_versions("nope", "1.0.0"), None);
}

#[test]
fn refuses_managed_installs() {
    assert!(managed_install(Path::new("/usr/bin/bitacora-cli"), false).is_some());
    assert!(
        managed_install(
            Path::new("/Applications/Bitacora.app/Contents/MacOS/x"),
            false
        )
        .is_some()
    );
    assert!(managed_install(Path::new("/nix/store/abc/bin/x"), false).is_some());
    assert!(managed_install(Path::new("/home/u/.local/bin/bitacora-cli"), true).is_some());
    assert!(managed_install(Path::new("/home/u/.local/bin/bitacora-cli"), false).is_none());
}

#[test]
fn digest_lookup() {
    let sums = "AB12  a.tar.gz\ncd34 *b.zip\n";
    assert_eq!(expected_digest(sums, "a.tar.gz").as_deref(), Some("ab12"));
    assert_eq!(expected_digest(sums, "b.zip").as_deref(), Some("cd34"));
    assert_eq!(expected_digest(sums, "c"), None);
}

#[test]
fn extracts_tar_and_zip() {
    assert_eq!(extract_binary("x.tar.gz", &targz(b"A")).expect("tar"), b"A");
    assert_eq!(extract_binary("x.zip", &zipped(b"B")).expect("zip"), b"B");
    assert!(extract_binary("x.rar", b"").is_err());
}

#[test]
fn updates_binary_after_checksum_verification() {
    let f = fixture("v9.9.9", false, false);
    let out = update(&args(&f.api), "0.1.0", &f.exe, TARGET).expect("update");
    assert_eq!(
        out,
        Outcome::Updated {
            from: "0.1.0".into(),
            to: "9.9.9".into()
        }
    );
    assert_eq!(std::fs::read(&f.exe).expect("read"), b"NEW-BINARY");
}

#[test]
fn rejects_checksum_mismatch_and_keeps_old_binary() {
    let f = fixture("v9.9.9", false, true);
    let err = update(&args(&f.api), "0.1.0", &f.exe, TARGET).expect_err("must fail");
    assert!(err.to_string().contains("checksum mismatch"), "{err:#}");
    assert_eq!(std::fs::read(&f.exe).expect("read"), b"OLD-BINARY");
}

#[test]
fn check_only_and_up_to_date() {
    let f = fixture("v9.9.9", false, false);
    let mut a = args(&f.api);
    a.check = true;
    assert_eq!(
        update(&a, "0.1.0", &f.exe, TARGET).expect("check"),
        Outcome::Available("9.9.9".into())
    );
    assert_eq!(std::fs::read(&f.exe).expect("read"), b"OLD-BINARY");
    assert_eq!(
        update(&args(&f.api), "9.9.9", &f.exe, TARGET).expect("same"),
        Outcome::UpToDate("9.9.9".into())
    );
}

#[test]
fn prereleases_need_the_beta_channel() {
    let f = fixture("v9.9.9-beta.1", true, false);
    assert!(matches!(
        update(&args(&f.api), "0.1.0", &f.exe, TARGET).expect("stable"),
        Outcome::UpToDate(_)
    ));
    let mut a = args(&f.api);
    a.prerelease = true;
    assert!(matches!(
        update(&a, "0.1.0", &f.exe, TARGET).expect("beta"),
        Outcome::Updated { .. }
    ));
}

#[test]
fn missing_asset_for_target_is_refused() {
    let f = fixture("v9.9.9", false, false);
    let err = update(&args(&f.api), "0.1.0", &f.exe, "other").expect_err("no asset");
    assert!(err.to_string().contains("no asset"), "{err:#}");
}
