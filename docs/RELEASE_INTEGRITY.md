# Reddart Release Integrity & Checksum Specification

**Document Version:** 1.0.0  
**Status:** Authoritative  
**Date:** November 12, 2026  

---

## 1. Executive Policy: Immutable Releases & Single Source of Truth

To ensure cryptographic reproducibility and prevent user-facing checksum discrepancies:

1. **Immutable Historical Tags:**  
   Git release tags (`v0.1.0`, `v0.2.0`, `v0.2.1`) are permanently immutable. They point to the exact git commit from which their release was published. Under no circumstance may an existing release tag be repointed, force-pushed, or retroactively overwritten with new binaries.

2. **Single Immutable Commit Per Release:**  
   Every future release candidate (e.g., `v0.3.0-rc1`) and production release (`v0.3.0`, `v1.0.0`) must be cut from a single, clean, passing commit on `master`.

3. **Authoritative Hash Matrix:**  
   For any release asset, four locations must agree byte-for-byte:
   - `ready_to_use/windows/SHA256SUMS.txt`
   - `ready_to_use/windows/manifest.json` (`sha256_executable` and `sha256_archive`)
   - `docs/RELEASE_NOTES_AND_LIMITATIONS.md`
   - `website/index.html` (the `<div class="checksum-grid">` block)

---

## 2. Historical Release Ledger & Audit Reconciliation

An audit of repository history identified why a checksum difference existed between the GitHub Release asset downloads and local repository manifests:

| Release / Asset | Commit | GitHub Release Status | Hash (SHA-256) | Notes |
| :--- | :--- | :--- | :--- | :--- |
| **`v0.2.1` Release Asset** (`Reddart.exe`) | `a4f576d` | Published `2026-11-07` | `377aaad62dc52f261003fd749ce398c30c11b2d97910b10e748f9086bdcd90d0` | Historical release binary published on GitHub Releases. |
| **`v0.2.1` Release Asset** (`Reddart-v0.2.1-Windows-x86_64.zip`) | `a4f576d` | Published `2026-11-07` | `40bcfbe045c48facb4294b2de037c98028897d309ad2f0e5d508d6ec9fe8a8b4` | Historical release ZIP archive published on GitHub Releases. |
| **Current Local Staging** (`Reddart.exe`) | `67f089d` | Local Staged | `914cbf52df15a921e7e6f2e0a1d20277ecd369c786cadd4615fc928b3319d16c` | Rebuilt post-v0.2.1 incorporating WAL durability flushes (`sync_all`) and MSRV 1.85 compliance. |
| **Current Local Staging** (`Reddart-v0.2.1-Windows-x86_64.zip`) | `67f089d` | Local Staged | `879055a466d700c1a96c3b53fd8e3f4d62f1423b555d2862aa973f2df546f1ac` | Rebuilt post-v0.2.1 with deterministic zip entry timestamps (`2026-01-01T00:00:00Z`). |

### Resolution Contract
- The historical release assets attached to GitHub tag `v0.2.1` remain unchanged as the published archive of that commit.
- The repository manifests in the working tree track the **local staged binaries** built from the current development branch.
- The upcoming milestone release will be tagged as **`v0.3.0`** upon completion of the Integrated Editing Workflow milestone, ensuring a clean break and strict 1:1 correspondence between git tag, release assets, and repository manifests.

---

## 3. Automated Verification

Integrity is enforced by:
1. `crates/rd-export/tests/release_integrity_test.rs`: Runs during `cargo test --workspace` and asserts hash parity across all 4 manifest files.
2. `scripts/verify_release_integrity.ps1`: Standalone script for pre-flight release checks.
