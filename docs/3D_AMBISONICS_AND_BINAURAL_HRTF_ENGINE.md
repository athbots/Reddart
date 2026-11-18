# 3D Ambisonics Spatial Audio Engine & Binaural HRTF Monitoring

**Document Reference:** RD-AUDIO-SPATIAL-2026-V1  
**Target:** Reddart Video Editor v1.5.0-dev (Phase V Milestone 26)  
**Status:** Implemented, Tested & Verified  
**MSRV:** Rust 1.85.0 (2024 Edition)  

---

## 1. Executive Summary & Acoustic Architecture

Reddart v1.5.0 introduces Milestone 26: an in-engine, pure-Rust **3D Ambisonics Spatial Audio Engine & Binaural Head-Related Transfer Function (HRTF) Monitoring Subsystem**. Designed for cinematic sound design, 360° VR/immersive video productions, and high-fidelity multichannel cinema mastering, this architecture provides complete spherical spatialization, realtime headphone virtualization, and surround bed decoding without third-party proprietary black boxes.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                 Reddart 3D Spatial Audio & Ambisonics Engine                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  [Audio Tracks / Timeline Stems]                                            │
│     │ (Mono / Stereo)                                                       │
│     ▼                                                                       │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ 3D Spatial Position & Distance Modeling                               │  │
│  │  - Spherical Coordinates (Azimuth θ, Elevation φ, Distance r)        │  │
│  │  - Distance Attenuation: 1 / (1 + α * (r - 1.0))                      │  │
│  │  - Air Absorption Filtering: 1-pole HF lowpass (humidity/distance)    │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│     │                                                                       │
│     ▼                                                                       │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ Spherical Harmonics Ambisonic Encoder                                 │  │
│  │  - Order 1 (4 ch), Order 2 (9 ch), Order 3 (16 ch)                    │  │
│  │  - ACN (Ambisonic Channel Number) ordering: n = ℓ² + ℓ + m            │  │
│  │  - Normalization: SN3D (AmbiX / YouTube VR), N3D, FuMa                │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│     │                                                                       │
│     ├───► [3D Soundfield Rotator (Yaw / Pitch / Roll Head-Tracking)]        │
│     │                                                                       │
│     ├───► [Binaural HRTF Engine (Headphone Virtualization)]                 │
│     │      - Woodworth-Schlosser Interaural Time Difference (ITD)           │
│     │      - Contralateral Head-Shadowing Filter (1-pole Lowpass)           │
│     │      - Pinna Spectral Elevation Notch (6.5 kHz - 9.5 kHz)             │
│     │                                                                       │
│     ├───► [Immersive Multichannel Decoder Matrix]                           │
│     │      - 7.1.4 Immersive Cinema Bed (L, R, C, LFE, Ls, Rs, Lb, Rb,     │
│     │                                    Tfl, Tfr, Tbl, Tbr)                │
│     │      - 7.1 / 5.1 Surround Downmix & Sub-bass LFE Extraction           │
│     │                                                                       │
│     └───► [Master Export Pipeline]                                          │
│            - YouTube 360 Spatial Audio (AmbiX 4-ch B-format, SN3D)          │
│            - Cinema Immersive 7.1.4 Bed (12-channel uncompressed WAV/AAC)   │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Mathematical Formulations & Spherical Harmonics

Ambisonics decomposes a 3D soundfield into orthogonal spherical harmonic functions defined over the unit sphere $\mathbb{S}^2$. A continuous sound pressure field $p(t, \theta, \phi)$ is represented as a series of spherical harmonic coefficients:

$$p(t, \theta, \phi) = \sum_{\ell=0}^{L} \sum_{m=-\ell}^{\ell} B_\ell^m(t) \, Y_\ell^m(\theta, \phi)$$

where $\ell$ denotes the spherical harmonic degree, $m$ is the azimuthal order ($-\ell \le m \le \ell$), and $L$ is the Ambisonic expansion order.

### 2.1 Ambisonic Channel Numbering (ACN)

Channels are indexed monotonically using the standard Ambisonic Channel Number:

$$n = \ell^2 + \ell + m$$

| Order ($L$) | Channels | ACN Indices ($n$) | Description |
|---|---|---|---|
| **1st Order** | 4 | 0–3 | Monopole ($W$), Dipoles ($Y, Z, X$) |
| **2nd Order** | 9 | 0–8 | Quadrupoles ($V, T, R, S, U$) |
| **3rd Order** | 16 | 0–15 | Octupoles ($Q, O, M, K, L, N, P, \dots$) |

### 2.2 Exact Real Spherical Harmonic Definitions

Reddart computes real spherical harmonics up to degree $\ell = 3$ evaluated for azimuth $\theta$ and elevation $\phi$:

#### Order 0 (Monopole):
- $n = 0 \quad (\ell=0, m=0): \quad Y_0^0 = 1.0$

#### Order 1 (Dipoles):
- $n = 1 \quad (\ell=1, m=-1): \quad Y_1^{-1} = \sin(\theta)\cos(\phi) \quad (Y \text{ / Left-Right})$
- $n = 2 \quad (\ell=1, m=0): \quad Y_1^{0} = \sin(\phi) \quad (Z \text{ / Up-Down})$
- $n = 3 \quad (\ell=1, m=1): \quad Y_1^{1} = \cos(\theta)\cos(\phi) \quad (X \text{ / Front-Back})$

#### Order 2 (Quadrupoles):
- $n = 4 \quad (\ell=2, m=-2): \quad Y_2^{-2} = \frac{\sqrt{3}}{2} \sin(2\theta)\cos^2(\phi)$
- $n = 5 \quad (\ell=2, m=-1): \quad Y_2^{-1} = \frac{\sqrt{3}}{2} \sin(\theta)\sin(2\phi)$
- $n = 6 \quad (\ell=2, m=0): \quad Y_2^{0} = \frac{1}{2} (3\sin^2(\phi) - 1)$
- $n = 7 \quad (\ell=2, m=1): \quad Y_2^{1} = \frac{\sqrt{3}}{2} \cos(\theta)\sin(2\phi)$
- $n = 8 \quad (\ell=2, m=2): \quad Y_2^{2} = \frac{\sqrt{3}}{2} \cos(2\theta)\cos^2(\phi)$

#### Order 3 (Octupoles):
- $n = 9 \dots 15$: Complete set of degree-3 real harmonics evaluated via orthonormal legendre polynomial recurrence.

### 2.3 Normalization Standards

Reddart provides lossless bidirectional conversions between standard normalization formats:
1. **SN3D (Schmidt Semi-Normalized):** Used by the AmbiX format (`.amb` / `.wav`), YouTube 360, and Google Spatial Audio. Maximum amplitude of any harmonic is bounded by $1.0$.
2. **N3D (Full 3D Normalization):** Satisfies continuous orthonormality over the sphere:
   $$\int_{\mathbb{S}^2} [Y_\ell^m(\theta, \phi)]^2 \, d\Omega = 4\pi$$
   Conversion factor from SN3D: $c(\ell, m) = \sqrt{2\ell + 1}$.
3. **FuMa (Furse-Malham):** Traditional 1st-order B-format standard ($W = \frac{1}{\sqrt{2}}$, $X, Y, Z$ unweighted).

---

## 3. 3D Soundfield Rotation & Head Tracking

Head-tracked binaural listening requires rotating the Ambisonic soundfield in real-time as the listener moves their head.

A soundfield rotation by Euler angles (Yaw $\psi$, Pitch $\vartheta$, Roll $\varphi$) applies an orthonormal coordinate rotation matrix $R(\psi, \vartheta, \varphi) \in SO(3)$ to the first-order Cartesian dipole triplet $[X, Y, Z]^T$:

$$\begin{bmatrix} X' \\ Y' \\ Z' \end{bmatrix} = \mathbf{R} \begin{bmatrix} X \\ Y \\ Z \end{bmatrix}$$

The Reddart `AmbisonicRotator` implements this transformation with zero memory allocations, preserving absolute soundfield energy ($\| [X', Y', Z'] \| = \| [X, Y, Z] \|$).

---

## 4. Binaural HRTF Synthesis Model

Headphones deliver sound directly to each ear canal, bypassing the natural acoustic diffraction created by the listener's head, torso, and pinnae. Reddart's `BinauralHrtfEngine` models these acoustic cues in real-time.

### 4.1 Woodworth-Schlosser Interaural Time Difference (ITD)

The delay $\tau(\theta)$ between the arrival of a wavefront at the ipsilateral ear and the contralateral ear around a spherical head of radius $a \approx 0.0875\text{ m}$ is governed by Woodworth's formula:

$$\tau(\theta) = \frac{a}{c} (\sin|\theta| + |\theta|)$$

where $c = 343\text{ m/s}$ is the speed of sound in air.
- Maximum contralateral delay at $\theta = \pm 90^\circ$ ($\frac{\pi}{2}$ rad):
  $$\tau_{\max} = \frac{0.0875}{343} \left(1 + \frac{\pi}{2}\right) \approx 0.655\text{ ms} \approx 31.5 \text{ samples at } 48\text{ kHz}$$
- Implemented using fractional ring-buffer delay interpolation.

### 4.2 Contralateral Head-Shadowing Filter

High frequencies are occluded by the acoustic shadow of the human cranium. Reddart computes an angle-dependent lowpass filter on the contralateral cross-feed channel:

$$f_c(\theta) = 20000.0 - (20000.0 - 1500.0) \cdot |\sin(\theta)|$$

At $\theta = 0$ (front), $f_c = 20\text{ kHz}$ (no attenuation). At $\theta = \pm 90^\circ$, $f_c = 1.5\text{ kHz}$ (deep shadowing).

### 4.3 Pinna Spectral Elevation Notch

To enable vertical localization (distinguishing sound coming from above vs. below), human pinnae introduce a destructive spectral interference notch between $6.5\text{ kHz}$ and $9.5\text{ kHz}$. Reddart computes:

$$f_{\text{notch}}(\phi) = 6500.0 + 3000.0 \cdot \left(\frac{\sin(\phi) + 1.0}{2.0}\right)$$

---

## 5. Multichannel Immersive Beds & Decoders

Reddart provides pre-computed, energy-balanced mode-matching decoders for surround and immersive cinema formats.

### 5.1 7.1.4 Immersive Bed Layout (12 Channels)

The 7.1.4 loudspeaker bed consists of 7 ear-level surround speakers, 1 LFE sub-bass channel, and 4 overhead height speakers:

| Channel | Label | Azimuth ($\theta$) | Elevation ($\phi$) |
|---|---|---|---|
| 0 | Front Left ($L$) | $-30^\circ$ | $0^\circ$ |
| 1 | Front Right ($R$) | $+30^\circ$ | $0^\circ$ |
| 2 | Center ($C$) | $0^\circ$ | $0^\circ$ |
| 3 | Low-Frequency Effects ($LFE$) | — | — |
| 4 | Left Surround ($Ls$) | $-90^\circ$ | $0^\circ$ |
| 5 | Right Surround ($Rs$) | $+90^\circ$ | $0^\circ$ |
| 6 | Left Back ($Lb$) | $-150^\circ$ | $0^\circ$ |
| 7 | Right Back ($Rb$) | $+150^\circ$ | $0^\circ$ |
| 8 | Top Front Left ($Tfl$) | $-45^\circ$ | $+45^\circ$ |
| 9 | Top Front Right ($Tfr$) | $+45^\circ$ | $+45^\circ$ |
| 10 | Top Back Left ($Tbl$) | $-135^\circ$ | $+45^\circ$ |
| 11 | Top Back Right ($Tbr$) | $+135^\circ$ | $+45^\circ$ |

### 5.2 Constant-Power Height Elevation Splitting

Energy across horizontal and vertical speaker planes is partitioned such that:

$$\text{Gain}_{\text{horizontal}} = \cos(\phi), \quad \text{Gain}_{\text{height}} = \sin(\phi)$$

Since $\cos^2(\phi) + \sin^2(\phi) = 1.0$, total acoustic radiated power remains invariant as a sound source moves overhead.

---

## 6. Export Presets & Standards Compliance

Reddart integrates Ambisonics directly into its export pipeline:

1. **`youtube_spatial_audio_360`:**
   - Container: MP4 or MOV.
   - Layout: `Ambisonics1stOrder` (4 channels).
   - Ordering: ACN ($W, Y, Z, X$).
   - Normalization: SN3D (AmbiX standard).
   - FFmpeg Mapping: `-ac 4` with spatial metadata container tagging.
2. **`cinema_immersive_714`:**
   - Container: MP4, MOV, or uncompressed multi-channel WAV.
   - Layout: `Surround714` (12 channels).
   - Discrete uncompressed PCM audio stream.

---

## 7. Verification & Parity Test Matrix

| Test Identifier | Coverage Area | Result |
|---|---|---|
| `test_ambisonic_order_acn_indexing` | ACN bidirectional index calculation ($n \leftrightarrow \ell, m$) | PASS |
| `test_spherical_harmonics_orthonormality` | Degree-1 & Degree-2 Legendre polynomial evaluation | PASS |
| `test_ambisonic_encoding_polarities` | Discrete directional encoding into $W, Y, Z, X$ | PASS |
| `test_ambisonic_soundfield_rotation` | 3D Euler rotation matrix conservation | PASS |
| `test_binaural_hrtf_pan_localization` | Woodworth ITD and head-shadowing attenuation | PASS |
| `test_714_immersive_decoder_energy_balance` | Constant-power elevation and surround decoding | PASS |
| `test_multichannel_mixer_714_routing` | 12-channel interleaved PCM audio rendering | PASS |
| `test_export_preset_spatial_audio_configuration` | Export preset layout mapping & FFmpeg channel count | PASS |
