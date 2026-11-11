# Reddart Git Branching & CI Integration Workflow

## 1. Branch Architecture

To maintain release stability and zero regressions in the Reddart desktop NLE, development adheres to a strict branching model:

```
[ master ] (Protected Release Branch - Tagged Releases Only)
    ▲
    │ (Squash / Rebase Pull Request, Verified CI & Reproducibility)
[ develop / integration/* ] (Integration Staging)
    ▲
    │ (Feature PRs)
[ feat/RED-xxx ] (Topic / Ticket Feature Branches)
```

### 1.1 Protected Branches
- **`master`**:
  - Contains authoritative production-grade code matching release tags.
  - Direct pushes are strictly forbidden.
  - All changes must arrive via pull request from an integration or feature branch.
  - Branch protection rules enforced on GitHub:
    - Require pull request reviews before merging.
    - Require status checks to pass before merging:
      - `Check & Lint (Windows)` (rustfmt, clippy with zero warnings, locked build)
      - `Windows App & Integration Suite` (enforced FFmpeg, 100% test pass, bit-for-bit reproducible packaging)
      - `Core Engine (ubuntu-latest)`
      - `Core Engine (macos-latest)`
    - Require branches to be up to date before merging.
    - Require signed commits where applicable.

- **`develop` / `integration/*`**:
  - Staging area for multi-ticket phases before cutting a release milestone.
  - Must always compile and pass the entire test suite.

- **`feat/*` (e.g. `feat/engineering-directive-phase1`)**:
  - Dedicated working branch for specific engineering directives and tickets.
  - Linear git history with descriptive conventional commit messages.

---

## 2. Pull Request & CI Gates

Every pull request must pass the automated GitHub Actions CI pipeline:

1. **Gate 1: Format & Linter Compliance**:
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings` (zero warnings allowed)
   - `cargo build --release --locked -p reddart-windows` (enforces pinned dependency resolution)

2. **Gate 2: Enforced FFmpeg Testing**:
   - Tests run with `CI="true"` and `REDDART_REQUIRE_FFMPEG="1"`.
   - Any test requiring media decoding or encoding (`sample_project_render_test`, `release_suite`) will hard-fail if FFmpeg/FFprobe is missing, eliminating silent test skipping in CI.

3. **Gate 3: Packaging & Reproducibility Gate**:
   - Runs `scripts/package_windows.ps1 -VerifyReproducibility`.
   - Proves two independent release builds yield identical SHA-256 archive hashes.
   - Validates that every media path in `Demo_Project.reddart` exists in the distribution directory.

---

## 3. Release Tagging Rules

- Existing release tags (e.g. `v0.1.0`, `v0.2.0`, `v0.2.1`) are immutable historical milestones. They must NEVER be deleted, overwritten, or force-updated.
- New releases require an explicit version bump in `Cargo.toml`, regeneration of release checksums, and a newly cut annotated git tag.
