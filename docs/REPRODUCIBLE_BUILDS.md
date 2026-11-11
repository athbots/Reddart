# Reproducible Release Builds Protocol

**Target Version:** Reddart v0.2.1+  
**Target Platform:** Windows x86_64 (`x86_64-pc-windows-msvc`)  
**Toolchain:** Rust 1.85.0+ (stable)  

---

## 1. Principles of Reproducibility

To ensure independent verification of Reddart binaries without third-party tampering, release builds must be bit-for-bit deterministic:

1. **Path Remapping:** Local build paths (e.g. `C:\Users\username\repo`) are stripped using `--remap-path-prefix`, guaranteeing that compiled symbol paths match regardless of where the repository is cloned.
2. **Deterministic Optimization:** Release profile sets `codegen-units = 1` and `opt-level = 3` to prevent non-deterministic code generation across parallel compiler workers.
3. **Locked Dependency Resolution:** Builds enforce `Cargo.lock` exact semantic versions via `cargo build --locked`.
4. **Normalized Archive Timestamps:** ZIP files embed deterministic file timestamps (`2026-01-01T00:00:00Z`), preventing filesystem creation/modification dates from altering archive checksums.

---

## 2. Step-by-Step Build Instructions

Execute the following in PowerShell from the repository root:

```powershell
# 1. Clean previous build artifacts
cargo clean

# 2. Package and validate Windows release
powershell -ExecutionPolicy Bypass -File scripts\package_windows.ps1

# 3. Verify generated SHA-256 checksums
Get-FileHash ready_to_use\windows\Reddart.exe -Algorithm SHA256
Get-FileHash ready_to_use\windows\Reddart-v0.2.1-Windows-x86_64.zip -Algorithm SHA256

# 4. Compare against recorded checksums
Get-Content ready_to_use\windows\SHA256SUMS.txt
```

---

## 3. Automated Verification via CI

GitHub Actions automatically executes the clean build and runs test suites across Windows, Ubuntu, and macOS runners on every push and pull request. The build log confirms:
- Exact toolchain version (`rustc 1.85+`)
- Zero Clippy warnings (`cargo clippy --all-targets --all-features -- -D warnings`)
- 100% passing tests (`cargo test --workspace`)
