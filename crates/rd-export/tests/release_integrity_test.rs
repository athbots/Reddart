//! Automated Release Integrity & Checksum Verification Test Suite
//!
//! Validates:
//! 1. All files in `ready_to_use/windows/SHA256SUMS.txt` exist and match their on-disk SHA-256.
//! 2. `ready_to_use/windows/manifest.json` sha256 properties match `SHA256SUMS.txt`.
//! 3. `docs/RELEASE_NOTES_AND_LIMITATIONS.md` contains the authoritative hashes.
//! 4. `website/index.html` contains the authoritative hashes.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Self-contained FIPS 180-4 standard SHA-256 digest function (zero external dependencies).
fn compute_sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h0: u32 = 0x6a09e667;
    let mut h1: u32 = 0xbb67ae85;
    let mut h2: u32 = 0x3c6ef372;
    let mut h3: u32 = 0xa54ff53a;
    let mut h4: u32 = 0x510e527f;
    let mut h5: u32 = 0x9b05688c;
    let mut h6: u32 = 0x1f83d9ab;
    let mut h7: u32 = 0x5be0cd19;

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;
        let mut f = h5;
        let mut g = h6;
        let mut h = h7;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
        h5 = h5.wrapping_add(f);
        h6 = h6.wrapping_add(g);
        h7 = h7.wrapping_add(h);
    }

    format!(
        "{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
        h0, h1, h2, h3, h4, h5, h6, h7
    )
}

fn compute_file_sha256(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    Ok(compute_sha256(&bytes))
}

#[test]
fn test_sha256_known_vector() {
    assert_eq!(
        compute_sha256(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        compute_sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn test_release_manifest_and_file_hash_integrity() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent().unwrap().parent().unwrap();

    let sums_path = root.join("ready_to_use/windows/SHA256SUMS.txt");
    assert!(
        sums_path.exists(),
        "SHA256SUMS.txt must exist at {:?}",
        sums_path
    );

    let sums_content = fs::read_to_string(&sums_path).expect("Read SHA256SUMS.txt");
    let mut expected_hashes: HashMap<String, String> = HashMap::new();

    for line in sums_content.lines() {
        let trimmed = line.trim().trim_start_matches('\u{feff}');
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        assert_eq!(
            parts.len(),
            2,
            "Each line in SHA256SUMS.txt must be '<hash> <filename>': {}",
            line
        );
        expected_hashes.insert(parts[1].to_string(), parts[0].to_lowercase());
    }

    let cargo_toml_str = fs::read_to_string(root.join("Cargo.toml")).expect("Read Cargo.toml");
    let version = cargo_toml_str
        .lines()
        .find(|l| l.trim().starts_with("version = "))
        .and_then(|l| l.split('"').nth(1))
        .unwrap_or("0.2.1");
    let zip_name = format!("Reddart-v{}-Windows-x86_64.zip", version);

    assert!(
        expected_hashes.contains_key("Reddart.exe"),
        "Must contain hash for Reddart.exe"
    );
    assert!(
        expected_hashes.contains_key(&zip_name),
        "Must contain hash for release archive: {}",
        zip_name
    );

    // 1. Verify on-disk file checksums
    for (filename, expected_hash) in &expected_hashes {
        let file_path = root.join("ready_to_use/windows").join(filename);
        assert!(
            file_path.exists(),
            "Tracked release file must exist on disk: {:?}",
            file_path
        );
        let actual_hash = compute_file_sha256(&file_path)
            .unwrap_or_else(|e| panic!("Failed to hash {:?}: {:?}", file_path, e));
        assert_eq!(
            actual_hash, *expected_hash,
            "Checksum mismatch for file '{}'. Expected {}, got {}",
            filename, expected_hash, actual_hash
        );
    }

    // 2. Verify manifest.json matches
    let manifest_path = root.join("ready_to_use/windows/manifest.json");
    assert!(manifest_path.exists());
    let manifest_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap())
            .expect("Valid JSON manifest");

    let exe_hash = expected_hashes.get("Reddart.exe").unwrap();
    let zip_hash = expected_hashes
        .get(&zip_name)
        .unwrap_or_else(|| panic!("Missing hash for {}", zip_name));

    assert_eq!(
        manifest_json["sha256_executable"].as_str().unwrap(),
        exe_hash,
        "manifest.json sha256_executable mismatch"
    );
    assert_eq!(
        manifest_json["sha256_archive"].as_str().unwrap(),
        zip_hash,
        "manifest.json sha256_archive mismatch"
    );

    // 3. Verify docs/RELEASE_NOTES_AND_LIMITATIONS.md
    let notes_path = root.join("docs/RELEASE_NOTES_AND_LIMITATIONS.md");
    assert!(notes_path.exists());
    let notes_content = fs::read_to_string(&notes_path).unwrap();
    assert!(
        notes_content.contains(exe_hash),
        "RELEASE_NOTES_AND_LIMITATIONS.md must contain executable hash {}",
        exe_hash
    );
    assert!(
        notes_content.contains(zip_hash),
        "RELEASE_NOTES_AND_LIMITATIONS.md must contain archive hash {}",
        zip_hash
    );

    // 4. Verify website/index.html
    let site_path = root.join("website/index.html");
    assert!(site_path.exists());
    let site_content = fs::read_to_string(&site_path).unwrap();
    assert!(
        site_content.contains(exe_hash),
        "website/index.html must contain executable hash {}",
        exe_hash
    );
    assert!(
        site_content.contains(zip_hash),
        "website/index.html must contain archive hash {}",
        zip_hash
    );
}

#[test]
fn test_cross_platform_packaging_integrity() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent().unwrap().parent().unwrap();

    // 1. Verify macOS ready-to-use artifacts
    let mac_manifest_path = root.join("ready_to_use/mac/manifest.json");
    assert!(
        mac_manifest_path.exists(),
        "ready_to_use/mac/manifest.json must exist"
    );
    let mac_manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&mac_manifest_path).unwrap())
            .expect("Valid JSON macOS manifest");

    assert_eq!(
        mac_manifest["product"].as_str().unwrap(),
        "Reddart Video Editor"
    );
    assert_eq!(mac_manifest["version"].as_str().unwrap(), "1.0.0");
    assert_eq!(mac_manifest["bundle"].as_str().unwrap(), "Reddart.app");
    assert_eq!(mac_manifest["effects_count"].as_u64().unwrap(), 11);

    let mac_zip_name = mac_manifest["archive"]["filename"].as_str().unwrap();
    let mac_zip_path = root.join("ready_to_use/mac").join(mac_zip_name);
    assert!(
        mac_zip_path.exists(),
        "macOS zip archive must exist at {:?}",
        mac_zip_path
    );

    let actual_mac_hash = compute_file_sha256(&mac_zip_path).expect("Hash macOS zip");
    assert_eq!(
        actual_mac_hash,
        mac_manifest["archive"]["sha256"].as_str().unwrap(),
        "macOS archive hash must match manifest"
    );

    let mac_sums_path = root.join("ready_to_use/mac/SHA256SUMS.txt");
    assert!(
        mac_sums_path.exists(),
        "ready_to_use/mac/SHA256SUMS.txt must exist"
    );
    let mac_sums_content = fs::read_to_string(&mac_sums_path).unwrap();
    assert!(
        mac_sums_content.contains(&actual_mac_hash),
        "macOS SHA256SUMS.txt must contain zip hash"
    );

    // 2. Verify Linux ready-to-use artifacts
    let linux_manifest_path = root.join("ready_to_use/linux/manifest.json");
    assert!(
        linux_manifest_path.exists(),
        "ready_to_use/linux/manifest.json must exist"
    );
    let linux_manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&linux_manifest_path).unwrap())
            .expect("Valid JSON Linux manifest");

    assert_eq!(
        linux_manifest["product"].as_str().unwrap(),
        "Reddart Video Editor"
    );
    assert_eq!(linux_manifest["version"].as_str().unwrap(), "1.0.0");
    assert_eq!(
        linux_manifest["desktop_entry"].as_str().unwrap(),
        "share/applications/reddart.desktop"
    );
    assert_eq!(linux_manifest["effects_count"].as_u64().unwrap(), 11);

    let linux_zip_name = linux_manifest["archive"]["filename"].as_str().unwrap();
    let linux_zip_path = root.join("ready_to_use/linux").join(linux_zip_name);
    assert!(
        linux_zip_path.exists(),
        "Linux zip archive must exist at {:?}",
        linux_zip_path
    );

    let actual_linux_hash = compute_file_sha256(&linux_zip_path).expect("Hash Linux zip");
    assert_eq!(
        actual_linux_hash,
        linux_manifest["archive"]["sha256"].as_str().unwrap(),
        "Linux archive hash must match manifest"
    );

    let linux_sums_path = root.join("ready_to_use/linux/SHA256SUMS.txt");
    assert!(
        linux_sums_path.exists(),
        "ready_to_use/linux/SHA256SUMS.txt must exist"
    );
    let linux_sums_content = fs::read_to_string(&linux_sums_path).unwrap();
    assert!(
        linux_sums_content.contains(&actual_linux_hash),
        "Linux SHA256SUMS.txt must contain zip hash"
    );

    // 3. Verify FreeDesktop specifications
    let desktop_file = root.join("platforms/linux/reddart.desktop");
    assert!(desktop_file.exists());
    let desktop_str = fs::read_to_string(&desktop_file).unwrap();
    assert!(desktop_str.contains("Type=Application"));
    assert!(desktop_str.contains("Name=Reddart"));
    assert!(desktop_str.contains("Exec=reddart %F"));

    let metainfo_file = root.join("platforms/linux/app.reddart.Reddart.metainfo.xml");
    assert!(metainfo_file.exists());
    let metainfo_str = fs::read_to_string(&metainfo_file).unwrap();
    assert!(metainfo_str.contains("<id>app.reddart.Reddart</id>"));

    let svg_icon = root.join("platforms/linux/reddart.svg");
    assert!(svg_icon.exists());
    assert!(!fs::read_to_string(&svg_icon).unwrap().is_empty());
}
