# Windows Distribution Lifecycle & Validation Protocol

**Document:** VAL-WIN-DIST-2026-V1  
**Target:** Reddart Video Editor v0.2.1+ (Windows x86_64)  
**Binary Location:** `ready_to_use/windows/Reddart.exe`  
**Distribution Archive:** `ready_to_use/windows/Reddart-v0.2.1-Windows-x86_64.zip`  
**Status:** Verified Operational Procedure  

---

## 1. Distribution Philosophy & Zero-Registry Architecture

Reddart follows a portable, zero-installation desktop design philosophy on Windows:
1. **Self-Contained Executable:** `Reddart.exe` embeds all core timeline, render graph, audio mixing, and UI logic within a single static executable (~10.5 MB).
2. **No Administrator Privileges Required:** Can be executed from user-space directories (`Downloads`, `Desktop`, or portable USB flash storage).
3. **Zero Required Registry Keys:** The application does not write to `HKEY_LOCAL_MACHINE` or create stealth background services or update daemons.
4. **Isolated User Configuration:** State and autosave files default to the standard user profile directory or relative to the opened project file.

---

## 2. Validation Test Procedure

```
┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│  Phase 1: Clean  │ ──▶ │ Phase 2: Launch  │ ──▶ │ Phase 3: In-Place│ ──▶ │ Phase 4: Clean   │
│   Extraction     │     │ & Direct3D Check │     │      Update      │     │   Uninstall      │
└──────────────────┘     └──────────────────┘     └──────────────────┘     └──────────────────┘
```

### Phase 1: Clean Installation & Extraction
- **Action:** Download or copy `Reddart-v0.2.1-Windows-x86_64.zip` into a clean sandbox directory (`C:\TestDist\Reddart`).
- **Validation Steps:**
  1. Extract using native Windows Explorer (`Expand-Archive -Path Reddart-v0.2.1-Windows-x86_64.zip -DestinationPath C:\TestDist\Reddart`).
  2. Verify SHA256 integrity using `ready_to_use\windows\SHA256SUMS.txt`:
     ```powershell
     Get-FileHash C:\TestDist\Reddart\Reddart.exe -Algorithm SHA256
     ```
  3. Ensure presence of all runtime collateral:
     - `Reddart.exe`
     - `VERSION.txt`
     - `README_QUICKSTART.txt`
     - `LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE`
     - `manifest.json`
     - `register_file_association.bat`
     - `unregister_file_association.bat`
     - `samples\Demo_Project.reddart`
     - `Reddart_Master_Demo.mp4`

### Phase 2: First Launch & Hardware Initialization
- **Action:** Launch `Reddart.exe` directly via file explorer or terminal:
  ```powershell
  .\Reddart.exe
  ```
- **Validation Criteria:**
  - Launch latency under 250 ms.
  - Direct3D 12 adapter initialized without DirectX runtime errors.
  - WASAPI 48 kHz stereo audio output device opened successfully.
  - Status bar confirms: `"Reddart Studio v0.2.1 | Press Space to preview, Ctrl+T for templates, Ctrl+E to export"`.
  - Pressing `Space` starts smooth 60 fps timeline preview with active audio VU meter.
  - Optional: File Association Registration:
    - Run `register_file_association.bat` (registers `HKCU\Software\Classes\.reddart`).
    - Double-clicking `Demo_Project.reddart` in Windows Explorer opens Reddart directly.

### Phase 3: In-Place Binary Update
- **Action:** Simulate upgrading from v0.2.0 to v0.2.1 or patching a new build.
- **Validation Criteria:**
  - Close `Reddart.exe`.
  - Overwrite `Reddart.exe` with the new release binary.
  - Relaunch `Reddart.exe`.
  - Verify project files, `.reddart.wal` journals, and user preferences load without schema conversion failures or data loss.

### Phase 4: Clean Uninstallation & Teardown
- **Action:** Complete removal of application and system traces.
- **Validation Steps:**
  1. If file associations were registered, run `unregister_file_association.bat`.
  2. Verify that registry key `HKCU\Software\Classes\.reddart` and `HKCU\Software\Classes\Reddart.Project` are deleted.
  3. Delete the extracted `Reddart` directory:
     ```powershell
     Remove-Item -Recurse -Force "C:\TestDist\Reddart"
     ```
  4. Verify system state: No background processes, no orphaned scheduled tasks, and no leftover files outside user project paths.

---

## 3. Automation Script

To package and validate Windows distribution builds automatically:
```powershell
powershell -ExecutionPolicy Bypass -File scripts\package_windows.ps1
```
The script runs the full Cargo release build, stages files, generates the zip archive, and updates `SHA256SUMS.txt`.

