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
| **`v0.3.0` Release Asset** (`Reddart.exe`) | `master` | Release Candidate | `05cf222e69b3628d510136fa5d7f56f18b91c81203aea0fdb7c7e0d2bc635f9a` | Integrated Editing Workflow, multi-layer compositing, async decoder, timeline audio mixer. |
| **`v0.3.0` Release Asset** (`Reddart-v0.3.0-Windows-x86_64.zip`) | `master` | Release Candidate | `e3bf1328b2278ad2640061d0bd344025bcaeec4afc3cf910472e69933c87e77d` | Standalone release distribution archive with sample media and SHA-256 integrity verification. |

### Resolution Contract
- The historical release assets attached to GitHub tag `v0.2.1` remain unchanged as the published archive of that commit.
- The repository manifests in the working tree track the **local staged binaries** built from the current development branch.
- The upcoming milestone release will be tagged as **`v0.3.0`** upon completion of the Integrated Editing Workflow milestone, ensuring a clean break and strict 1:1 correspondence between git tag, release assets, and repository manifests.

---

## 3. Automated Verification

Integrity is enforced by:
1. `crates/rd-export/tests/release_integrity_test.rs`: Runs during `cargo test --workspace` and asserts hash parity across all 4 manifest files.
2. `scripts/verify_release_integrity.ps1`: Standalone script for pre-flight release checks.
