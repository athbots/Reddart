//! OpenColorIO (OCIO) Configuration Parser, Reference ACES Profiles & Color Pipeline Resolver.
//!
//! Provides production-grade OCIO v1/v2 configuration parsing, display/view mapping,
//! reference ACES 1.2/1.3 profile definitions, and dynamic resolution of color transform
//! chains for cinema, broadcast HDR, and VFX pipelines.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

use crate::color_space::{ColorGamut, ColorPipelineConfig, TransferFunction};

/// Errors encountered during OCIO configuration parsing and pipeline resolution.
#[derive(Debug, Error, PartialEq)]
pub enum OcioError {
    #[error("Failed to parse OCIO configuration: {0}")]
    ParseError(String),

    #[error("ColorSpace '{0}' was not found in OCIO config")]
    ColorSpaceNotFound(String),

    #[error("Display '{0}' was not found in OCIO config")]
    DisplayNotFound(String),

    #[error("View '{0}' was not found under display '{1}'")]
    ViewNotFound(String, String),

    #[error("Role '{0}' was not found in OCIO config")]
    RoleNotFound(String),

    #[error("Invalid transformation definition: {0}")]
    InvalidTransform(String),

    #[error("I/O error reading OCIO configuration: {0}")]
    Io(String),
}

/// Bit-depth specifications recognized by OpenColorIO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OcioBitDepth {
    #[default]
    Bit8,
    Bit10,
    Bit12,
    Bit16,
    Bit16f,
    Bit32f,
    Unknown,
}

impl OcioBitDepth {
    /// Parse an OCIO bitdepth string (e.g. "8i", "10i", "12i", "16f", "32f").
    pub fn parse_str(s: &str) -> Self {
        let trimmed = s.trim().to_lowercase();
        match trimmed.as_str() {
            "8ui" | "8i" | "8" => Self::Bit8,
            "10ui" | "10i" | "10" => Self::Bit10,
            "12ui" | "12i" | "12" => Self::Bit12,
            "16ui" | "16i" | "16" => Self::Bit16,
            "16f" | "half" => Self::Bit16f,
            "32f" | "float" => Self::Bit32f,
            _ => Self::Unknown,
        }
    }

    /// Return canonical OCIO bitdepth string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bit8 => "8i",
            Self::Bit10 => "10i",
            Self::Bit12 => "12i",
            Self::Bit16 => "16i",
            Self::Bit16f => "16f",
            Self::Bit32f => "32f",
            Self::Unknown => "unknown",
        }
    }

    /// Returns true if this bitdepth represents IEEE floating point values.
    pub fn is_floating_point(&self) -> bool {
        matches!(self, Self::Bit16f | Self::Bit32f)
    }
}

/// OpenColorIO Transform specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OcioTransform {
    Matrix { matrix: [f32; 16], offset: [f32; 4] },
    Exponent { value: [f32; 4] },
    File { src: String, interpolation: String },
    ColorSpace { src: String, dst: String },
}

/// Color space entry defined within an OCIO configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcioColorSpace {
    pub name: String,
    pub family: String,
    pub equalitygroup: String,
    pub bitdepth: OcioBitDepth,
    pub isdata: bool,
    pub description: String,
    pub to_reference: Option<OcioTransform>,
    pub from_reference: Option<OcioTransform>,
    pub gamut: Option<ColorGamut>,
    pub transfer_function: Option<TransferFunction>,
}

impl Default for OcioColorSpace {
    fn default() -> Self {
        Self {
            name: "ACES2065-1".to_string(),
            family: "ACES".to_string(),
            equalitygroup: String::new(),
            bitdepth: OcioBitDepth::Bit32f,
            isdata: false,
            description: "Default ACES2065-1 archival linear color space".to_string(),
            to_reference: None,
            from_reference: None,
            gamut: Some(ColorGamut::ACES2065),
            transfer_function: Some(TransferFunction::Linear),
        }
    }
}

impl OcioColorSpace {
    /// Create a new color space entry.
    pub fn new(
        name: impl Into<String>,
        family: impl Into<String>,
        bitdepth: OcioBitDepth,
        gamut: Option<ColorGamut>,
        tf: Option<TransferFunction>,
    ) -> Self {
        let name_str = name.into();
        Self {
            name: name_str.clone(),
            family: family.into(),
            equalitygroup: String::new(),
            bitdepth,
            isdata: false,
            description: format!("ColorSpace {}", name_str),
            to_reference: None,
            from_reference: None,
            gamut,
            transfer_function: tf,
        }
    }
}

/// Display view definition (e.g. "Standard", "ACES 1.0 - SDR Video", "HDR10").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcioView {
    pub name: String,
    pub colorspace: String,
    pub looks: Option<String>,
    pub rule: Option<String>,
}

impl OcioView {
    pub fn new(name: impl Into<String>, colorspace: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            colorspace: colorspace.into(),
            looks: None,
            rule: None,
        }
    }
}

/// Complete OpenColorIO configuration profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcioConfig {
    pub ocio_profile_version: u32,
    pub environment: HashMap<String, String>,
    pub search_path: Option<String>,
    pub roles: HashMap<String, String>,
    pub displays: HashMap<String, Vec<OcioView>>,
    pub active_displays: Vec<String>,
    pub active_views: Vec<String>,
    pub colorspaces: Vec<OcioColorSpace>,
}

impl Default for OcioConfig {
    fn default() -> Self {
        Self::aces_default()
    }
}

impl OcioConfig {
    /// Create an empty OCIO configuration profile.
    pub fn new() -> Self {
        Self {
            ocio_profile_version: 2,
            environment: HashMap::new(),
            search_path: None,
            roles: HashMap::new(),
            displays: HashMap::new(),
            active_displays: Vec::new(),
            active_views: Vec::new(),
            colorspaces: Vec::new(),
        }
    }

    /// Built-in Academy Color Encoding System (ACES 1.2 / 1.3) reference configuration.
    ///
    /// Provides standardized digital cinema color spaces:
    /// - ACES2065-1 (AP0 linear reference)
    /// - ACEScg (AP1 linear working space)
    /// - ACEScc & ACEScct (AP1 log color grading)
    /// - Rec.709 & sRGB (broadcast SDR)
    /// - Display P3 (wide color gamut)
    /// - Rec.2020 PQ & HLG (UHD HDR broadcast & mastering)
    /// - Raw (bypass)
    pub fn aces_default() -> Self {
        let mut config = Self::new();
        config.ocio_profile_version = 2;
        config.search_path = Some("luts".to_string());

        // Standard ACES roles
        config
            .roles
            .insert("reference".to_string(), "ACES2065-1".to_string());
        config
            .roles
            .insert("scene_linear".to_string(), "ACEScg".to_string());
        config
            .roles
            .insert("color_timing".to_string(), "ACEScc".to_string());
        config
            .roles
            .insert("compositing_log".to_string(), "ACEScct".to_string());
        config.roles.insert("data".to_string(), "Raw".to_string());
        config
            .roles
            .insert("default".to_string(), "ACES2065-1".to_string());
        config
            .roles
            .insert("rendering".to_string(), "ACEScg".to_string());
        config
            .roles
            .insert("color_picking".to_string(), "sRGB".to_string());
        config
            .roles
            .insert("texture_paint".to_string(), "sRGB".to_string());

        // Standard Displays and Views
        config.displays.insert(
            "sRGB".to_string(),
            vec![
                OcioView::new("Standard", "sRGB"),
                OcioView::new("ACES 1.0 - SDR Video", "Rec.709"),
                OcioView::new("Raw", "Raw"),
            ],
        );

        config.displays.insert(
            "Display P3".to_string(),
            vec![
                OcioView::new("Standard", "Display P3"),
                OcioView::new("Raw", "Raw"),
            ],
        );

        config.displays.insert(
            "Rec.2020".to_string(),
            vec![
                OcioView::new("HDR10 - 1000 nits", "Rec.2020 - PQ"),
                OcioView::new("HDR10 - 4000 nits", "Rec.2020 - PQ"),
                OcioView::new("HLG - Broadcast", "Rec.2020 - HLG"),
                OcioView::new("Raw", "Raw"),
            ],
        );

        config.active_displays = vec![
            "sRGB".to_string(),
            "Display P3".to_string(),
            "Rec.2020".to_string(),
        ];
        config.active_views = vec![
            "Standard".to_string(),
            "ACES 1.0 - SDR Video".to_string(),
            "HDR10 - 1000 nits".to_string(),
            "HLG - Broadcast".to_string(),
            "Raw".to_string(),
        ];

        // Standard Color Spaces
        config.colorspaces = vec![
            OcioColorSpace {
                name: "ACES2065-1".to_string(),
                family: "ACES".to_string(),
                equalitygroup: String::new(),
                bitdepth: OcioBitDepth::Bit32f,
                isdata: false,
                description: "ACES2065-1 linear exchange color space (AP0)".to_string(),
                to_reference: None,
                from_reference: None,
                gamut: Some(ColorGamut::ACES2065),
                transfer_function: Some(TransferFunction::Linear),
            },
            OcioColorSpace {
                name: "ACEScg".to_string(),
                family: "ACES".to_string(),
                equalitygroup: String::new(),
                bitdepth: OcioBitDepth::Bit32f,
                isdata: false,
                description: "ACEScg linear working color space (AP1)".to_string(),
                to_reference: None,
                from_reference: None,
                gamut: Some(ColorGamut::ACEScg),
                transfer_function: Some(TransferFunction::Linear),
            },
            OcioColorSpace {
                name: "ACEScc".to_string(),
                family: "ACES".to_string(),
                equalitygroup: String::new(),
                bitdepth: OcioBitDepth::Bit32f,
                isdata: false,
                description: "ACEScc logarithmic color grading space".to_string(),
                to_reference: None,
                from_reference: None,
                gamut: Some(ColorGamut::ACEScg),
                transfer_function: Some(TransferFunction::ACEScc),
            },
            OcioColorSpace {
                name: "ACEScct".to_string(),
                family: "ACES".to_string(),
                equalitygroup: String::new(),
                bitdepth: OcioBitDepth::Bit32f,
                isdata: false,
                description: "ACEScct logarithmic color grading space with toe".to_string(),
                to_reference: None,
                from_reference: None,
                gamut: Some(ColorGamut::ACEScg),
                transfer_function: Some(TransferFunction::ACEScct),
            },
            OcioColorSpace {
                name: "Rec.709".to_string(),
                family: "Displays".to_string(),
                equalitygroup: String::new(),
                bitdepth: OcioBitDepth::Bit8,
                isdata: false,
                description: "ITU-R BT.709 broadcast standard curve".to_string(),
                to_reference: None,
                from_reference: None,
                gamut: Some(ColorGamut::Rec709),
                transfer_function: Some(TransferFunction::Rec709),
            },
            OcioColorSpace {
                name: "sRGB".to_string(),
                family: "Displays".to_string(),
                equalitygroup: String::new(),
                bitdepth: OcioBitDepth::Bit8,
                isdata: false,
                description: "sRGB IEC 61966-2-1 standard computer display".to_string(),
                to_reference: None,
                from_reference: None,
                gamut: Some(ColorGamut::Rec709),
                transfer_function: Some(TransferFunction::SRGB),
            },
            OcioColorSpace {
                name: "Display P3".to_string(),
                family: "Displays".to_string(),
                equalitygroup: String::new(),
                bitdepth: OcioBitDepth::Bit10,
                isdata: false,
                description: "DCI-P3 wide color gamut display".to_string(),
                to_reference: None,
                from_reference: None,
                gamut: Some(ColorGamut::DisplayP3),
                transfer_function: Some(TransferFunction::SRGB),
            },
            OcioColorSpace {
                name: "Rec.2020 - PQ".to_string(),
                family: "HDR Displays".to_string(),
                equalitygroup: String::new(),
                bitdepth: OcioBitDepth::Bit10,
                isdata: false,
                description: "ITU-R BT.2020 SMPTE ST 2084 PQ (HDR10 10,000 nits mastering)"
                    .to_string(),
                to_reference: None,
                from_reference: None,
                gamut: Some(ColorGamut::Rec2020),
                transfer_function: Some(TransferFunction::PQ),
            },
            OcioColorSpace {
                name: "Rec.2020 - HLG".to_string(),
                family: "HDR Displays".to_string(),
                equalitygroup: String::new(),
                bitdepth: OcioBitDepth::Bit10,
                isdata: false,
                description: "ITU-R BT.2020 ARIB STD-B67 Hybrid Log-Gamma broadcast HDR"
                    .to_string(),
                to_reference: None,
                from_reference: None,
                gamut: Some(ColorGamut::Rec2020),
                transfer_function: Some(TransferFunction::HLG),
            },
            OcioColorSpace {
                name: "Raw".to_string(),
                family: "Utility".to_string(),
                equalitygroup: String::new(),
                bitdepth: OcioBitDepth::Bit32f,
                isdata: true,
                description: "Raw non-color data pass-through".to_string(),
                to_reference: None,
                from_reference: None,
                gamut: Some(ColorGamut::Rec709),
                transfer_function: Some(TransferFunction::Linear),
            },
        ];

        config
    }

    /// Load an OCIO configuration from a file on disk.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, OcioError> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            OcioError::Io(format!("Failed to read {}: {}", path.as_ref().display(), e))
        })?;
        Self::from_yaml_str(&content)
    }

    /// Parse an OCIO YAML configuration string.
    ///
    /// Robustly supports both standard OCIO v1 and v2 syntax, stripping custom YAML tags
    /// (`!<ColorSpace>`, `!<View>`, `!<MatrixTransform>`, etc.) and mapping sections into memory.
    pub fn from_yaml_str(input: &str) -> Result<Self, OcioError> {
        let mut config = Self::new();

        // State machine for section parsing
        #[derive(Debug, PartialEq)]
        enum Section {
            None,
            Roles,
            Displays,
            ColorSpaces,
            Environment,
        }

        let mut current_section = Section::None;
        let mut current_display: Option<String> = None;
        let mut current_cs: Option<OcioColorSpace> = None;

        for line in input.lines() {
            // Strip comments
            let uncommented = if let Some(idx) = line.find('#') {
                &line[..idx]
            } else {
                line
            };

            let trimmed = uncommented.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Top-level section switches (0 indentation)
            let is_top_level = !uncommented.starts_with(' ') && !uncommented.starts_with('\t');
            if is_top_level {
                if let Some(cs) = current_cs.take() {
                    config.colorspaces.push(cs);
                }

                if let Some(rest) = trimmed.strip_prefix("ocio_profile_version:") {
                    config.ocio_profile_version = rest.trim().parse::<u32>().unwrap_or(2);
                    current_section = Section::None;
                    continue;
                } else if let Some(rest) = trimmed.strip_prefix("search_path:") {
                    config.search_path = Some(clean_string_val(rest));
                    current_section = Section::None;
                    continue;
                } else if trimmed.starts_with("roles:") {
                    current_section = Section::Roles;
                    continue;
                } else if trimmed.starts_with("displays:") {
                    current_section = Section::Displays;
                    continue;
                } else if trimmed.starts_with("colorspaces:") {
                    current_section = Section::ColorSpaces;
                    continue;
                } else if trimmed.starts_with("environment:") {
                    current_section = Section::Environment;
                    continue;
                } else if trimmed.ends_with(':') {
                    current_section = Section::None;
                    continue;
                }
            }

            match current_section {
                Section::Roles => {
                    if let Some((role, target)) = parse_key_value(trimmed) {
                        config.roles.insert(role, target);
                    }
                }
                Section::Displays => {
                    // Check if this defines a display name (e.g. "sRGB:")
                    let leading_spaces = uncommented.len() - uncommented.trim_start().len();
                    if leading_spaces <= 4 && trimmed.ends_with(':') && !trimmed.starts_with('-') {
                        let disp_name = clean_string_val(trimmed.trim_end_matches(':'));
                        current_display = Some(disp_name.clone());
                        config.displays.entry(disp_name).or_default();
                    } else if trimmed.starts_with('-') {
                        // View definition: e.g. - !<View> {name: Standard, colorspace: sRGB}
                        if let Some(ref disp) = current_display {
                            if let Some(view) = parse_view_line(trimmed) {
                                config.displays.entry(disp.clone()).or_default().push(view);
                            }
                        }
                    }
                }
                Section::ColorSpaces => {
                    if trimmed.starts_with('-') {
                        // Start of new ColorSpace
                        if let Some(cs) = current_cs.take() {
                            config.colorspaces.push(cs);
                        }

                        let mut cs = OcioColorSpace {
                            name: String::new(),
                            family: String::new(),
                            equalitygroup: String::new(),
                            bitdepth: OcioBitDepth::Bit32f,
                            isdata: false,
                            description: String::new(),
                            to_reference: None,
                            from_reference: None,
                            gamut: None,
                            transfer_function: None,
                        };

                        // Check if name is defined inline
                        let line_rem = trimmed.trim_start_matches('-').trim();
                        let line_clean = remove_ocio_tag(line_rem);
                        if let Some((k, v)) = parse_key_value(line_clean) {
                            apply_cs_field(&mut cs, &k, &v);
                        }
                        current_cs = Some(cs);
                    } else if let Some(ref mut cs) = current_cs {
                        let line_clean = remove_ocio_tag(trimmed);
                        if let Some((k, v)) = parse_key_value(line_clean) {
                            apply_cs_field(cs, &k, &v);
                        }
                    }
                }
                Section::Environment => {
                    if let Some((k, v)) = parse_key_value(trimmed) {
                        config.environment.insert(k, v);
                    }
                }
                Section::None => {}
            }
        }

        if let Some(cs) = current_cs.take() {
            config.colorspaces.push(cs);
        }

        // Infer active displays & views if empty
        if config.active_displays.is_empty() {
            config.active_displays = config.displays.keys().cloned().collect();
            config.active_displays.sort();
        }

        // Auto-infer gamuts and transfer functions for standard color spaces
        for cs in &mut config.colorspaces {
            infer_color_space_attributes(cs);
        }

        Ok(config)
    }

    /// Serialize configuration to a canonical OCIO v2 YAML string.
    pub fn to_yaml_str(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "ocio_profile_version: {}\n\n",
            self.ocio_profile_version
        ));

        if let Some(ref sp) = self.search_path {
            out.push_str(&format!("search_path: \"{}\"\n\n", sp));
        }

        if !self.roles.is_empty() {
            out.push_str("roles:\n");
            let mut sorted_roles: Vec<_> = self.roles.iter().collect();
            sorted_roles.sort_by_key(|(k, _)| (*k).clone());
            for (role, target) in sorted_roles {
                out.push_str(&format!("  {}: {}\n", role, target));
            }
            out.push('\n');
        }

        if !self.displays.is_empty() {
            out.push_str("displays:\n");
            let mut sorted_displays: Vec<_> = self.displays.iter().collect();
            sorted_displays.sort_by_key(|(k, _)| (*k).clone());
            for (disp, views) in sorted_displays {
                out.push_str(&format!("  {}:\n", disp));
                for view in views {
                    out.push_str(&format!(
                        "    - !<View> {{name: {}, colorspace: {}}}\n",
                        view.name, view.colorspace
                    ));
                }
            }
            out.push('\n');
        }

        if !self.colorspaces.is_empty() {
            out.push_str("colorspaces:\n");
            for cs in &self.colorspaces {
                out.push_str("  - !<ColorSpace>\n");
                out.push_str(&format!("    name: {}\n", cs.name));
                if !cs.family.is_empty() {
                    out.push_str(&format!("    family: {}\n", cs.family));
                }
                out.push_str(&format!("    bitdepth: {}\n", cs.bitdepth.as_str()));
                out.push_str(&format!("    isdata: {}\n", cs.isdata));
                if !cs.description.is_empty() {
                    out.push_str(&format!("    description: \"{}\"\n", cs.description));
                }
                out.push('\n');
            }
        }

        out
    }

    /// Search for a color space by name or role alias.
    pub fn find_colorspace(&self, name_or_role: &str) -> Option<&OcioColorSpace> {
        let target_name = self
            .roles
            .get(name_or_role)
            .map(|s| s.as_str())
            .unwrap_or(name_or_role);
        self.colorspaces
            .iter()
            .find(|cs| cs.name.eq_ignore_ascii_case(target_name))
    }

    /// Retrieve the concrete colorspace corresponding to a specific role.
    pub fn get_role_target(&self, role: &str) -> Option<&str> {
        self.roles.get(role).map(|s| s.as_str())
    }

    /// Get views defined under a given display device.
    pub fn get_display_views(&self, display: &str) -> Option<&[OcioView]> {
        self.displays.get(display).map(|v| v.as_slice())
    }

    /// Resolve the target colorspace defined by a (display, view) combination.
    pub fn resolve_display_view(
        &self,
        display: &str,
        view: &str,
    ) -> Result<&OcioColorSpace, OcioError> {
        let views = self
            .displays
            .get(display)
            .ok_or_else(|| OcioError::DisplayNotFound(display.to_string()))?;

        let view_entry = views
            .iter()
            .find(|v| v.name.eq_ignore_ascii_case(view))
            .ok_or_else(|| OcioError::ViewNotFound(view.to_string(), display.to_string()))?;

        self.find_colorspace(&view_entry.colorspace)
            .ok_or_else(|| OcioError::ColorSpaceNotFound(view_entry.colorspace.clone()))
    }

    /// Resolve an end-to-end ColorPipelineConfig given a source colorspace and target display/view.
    pub fn resolve_pipeline(
        &self,
        src_name_or_role: &str,
        display: &str,
        view: &str,
    ) -> Result<ColorPipelineConfig, OcioError> {
        let src_cs = self
            .find_colorspace(src_name_or_role)
            .ok_or_else(|| OcioError::ColorSpaceNotFound(src_name_or_role.to_string()))?;

        let dst_cs = self.resolve_display_view(display, view)?;

        let src_gamut = src_cs.gamut.unwrap_or(ColorGamut::ACEScg);
        let src_tf = src_cs.transfer_function.unwrap_or(TransferFunction::Linear);
        let dst_gamut = dst_cs.gamut.unwrap_or(ColorGamut::Rec709);
        let dst_tf = dst_cs.transfer_function.unwrap_or(TransferFunction::SRGB);

        // Automatic tone mapping heuristic: enable filmic tone mapping if source is scene-referred
        // and target is standard dynamic range (sRGB or Rec.709) or explicitly designated as SDR.
        let is_sdr_target = matches!(dst_tf, TransferFunction::SRGB | TransferFunction::Rec709);
        let is_scene_linear_src = matches!(
            src_tf,
            TransferFunction::Linear | TransferFunction::ACEScc | TransferFunction::ACEScct
        );
        let should_tone_map = is_scene_linear_src
            && is_sdr_target
            && (view.contains("SDR") || view.contains("Standard"));

        Ok(ColorPipelineConfig {
            input_gamut: src_gamut,
            input_tf: src_tf,
            output_gamut: dst_gamut,
            output_tf: dst_tf,
            apply_acescc_cdl: false,
            cdl: Default::default(),
            apply_tone_mapping: should_tone_map,
            exposure_ev: 0.0,
            temperature_kelvin_delta: 0.0,
        })
    }

    /// Resolve an end-to-end ColorPipelineConfig directly between two color spaces.
    pub fn resolve_transform_chain(
        &self,
        src_name_or_role: &str,
        dst_name_or_role: &str,
    ) -> Result<ColorPipelineConfig, OcioError> {
        let src_cs = self
            .find_colorspace(src_name_or_role)
            .ok_or_else(|| OcioError::ColorSpaceNotFound(src_name_or_role.to_string()))?;

        let dst_cs = self
            .find_colorspace(dst_name_or_role)
            .ok_or_else(|| OcioError::ColorSpaceNotFound(dst_name_or_role.to_string()))?;

        let src_gamut = src_cs.gamut.unwrap_or(ColorGamut::ACEScg);
        let src_tf = src_cs.transfer_function.unwrap_or(TransferFunction::Linear);
        let dst_gamut = dst_cs.gamut.unwrap_or(ColorGamut::Rec709);
        let dst_tf = dst_cs.transfer_function.unwrap_or(TransferFunction::SRGB);

        Ok(ColorPipelineConfig {
            input_gamut: src_gamut,
            input_tf: src_tf,
            output_gamut: dst_gamut,
            output_tf: dst_tf,
            apply_acescc_cdl: false,
            cdl: Default::default(),
            apply_tone_mapping: false,
            exposure_ev: 0.0,
            temperature_kelvin_delta: 0.0,
        })
    }
}

// -----------------------------------------------------------------------------
// Internal Parsing Helpers
// -----------------------------------------------------------------------------

fn clean_string_val(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn remove_ocio_tag(s: &str) -> &str {
    let trimmed = s.trim();
    if let Some(idx) = trimmed.find("!<") {
        if let Some(end_idx) = trimmed[idx..].find('>') {
            let tag_end = idx + end_idx + 1;
            return trimmed[tag_end..].trim();
        }
    }
    trimmed
}

fn parse_key_value(line: &str) -> Option<(String, String)> {
    let clean = remove_ocio_tag(line);
    let colon_idx = clean.find(':')?;
    let key = clean[..colon_idx].trim().to_string();
    let val = clean_string_val(&clean[colon_idx + 1..]);
    if key.is_empty() {
        None
    } else {
        Some((key, val))
    }
}

fn parse_view_line(line: &str) -> Option<OcioView> {
    // Examples:
    // - !<View> {name: Standard, colorspace: sRGB}
    // - {name: Standard, colorspace: sRGB}
    let content = if let Some(open) = line.find('{') {
        let close = line.rfind('}')?;
        &line[open + 1..close]
    } else {
        line.trim_start_matches('-').trim()
    };

    let mut name = String::new();
    let mut colorspace = String::new();

    for part in content.split(',') {
        if let Some((k, v)) = parse_key_value(part.trim()) {
            match k.to_lowercase().as_str() {
                "name" => name = v,
                "colorspace" => colorspace = v,
                _ => {}
            }
        }
    }

    if !name.is_empty() && !colorspace.is_empty() {
        Some(OcioView::new(name, colorspace))
    } else {
        None
    }
}

fn apply_cs_field(cs: &mut OcioColorSpace, key: &str, val: &str) {
    match key.to_lowercase().as_str() {
        "name" => cs.name = val.to_string(),
        "family" => cs.family = val.to_string(),
        "equalitygroup" => cs.equalitygroup = val.to_string(),
        "bitdepth" => cs.bitdepth = OcioBitDepth::parse_str(val),
        "isdata" => cs.isdata = val.eq_ignore_ascii_case("true") || val == "1",
        "description" => cs.description = val.to_string(),
        _ => {}
    }
}

fn infer_color_space_attributes(cs: &mut OcioColorSpace) {
    if cs.gamut.is_some() && cs.transfer_function.is_some() {
        return;
    }

    let lower = cs.name.to_lowercase();
    if lower.contains("aces2065") || lower.contains("ap0") {
        cs.gamut = Some(ColorGamut::ACES2065);
        cs.transfer_function = Some(TransferFunction::Linear);
    } else if lower.contains("acescct") {
        cs.gamut = Some(ColorGamut::ACEScg);
        cs.transfer_function = Some(TransferFunction::ACEScct);
    } else if lower.contains("acescc") {
        cs.gamut = Some(ColorGamut::ACEScg);
        cs.transfer_function = Some(TransferFunction::ACEScc);
    } else if lower.contains("acescg") || lower.contains("ap1") {
        cs.gamut = Some(ColorGamut::ACEScg);
        cs.transfer_function = Some(TransferFunction::Linear);
    } else if lower.contains("rec.2020") || lower.contains("rec2020") || lower.contains("bt.2020") {
        cs.gamut = Some(ColorGamut::Rec2020);
        if lower.contains("pq") || lower.contains("2084") || lower.contains("hdr10") {
            cs.transfer_function = Some(TransferFunction::PQ);
        } else if lower.contains("hlg") || lower.contains("b67") {
            cs.transfer_function = Some(TransferFunction::HLG);
        } else {
            cs.transfer_function = Some(TransferFunction::Linear);
        }
    } else if lower.contains("p3") {
        cs.gamut = Some(ColorGamut::DisplayP3);
        if lower.contains("linear") {
            cs.transfer_function = Some(TransferFunction::Linear);
        } else {
            cs.transfer_function = Some(TransferFunction::SRGB);
        }
    } else if lower.contains("rec.709") || lower.contains("rec709") || lower.contains("bt.709") {
        cs.gamut = Some(ColorGamut::Rec709);
        cs.transfer_function = Some(TransferFunction::Rec709);
    } else if lower.contains("srgb") {
        cs.gamut = Some(ColorGamut::Rec709);
        cs.transfer_function = Some(TransferFunction::SRGB);
    } else if lower.contains("raw") {
        cs.gamut = Some(ColorGamut::Rec709);
        cs.transfer_function = Some(TransferFunction::Linear);
        cs.isdata = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_built_in_aces_default_config() {
        let config = OcioConfig::aces_default();
        assert_eq!(config.ocio_profile_version, 2);
        assert_eq!(config.get_role_target("scene_linear"), Some("ACEScg"));
        assert_eq!(config.get_role_target("color_timing"), Some("ACEScc"));
        assert_eq!(config.get_role_target("reference"), Some("ACES2065-1"));

        let acescg = config
            .find_colorspace("scene_linear")
            .expect("scene_linear should resolve");
        assert_eq!(acescg.name, "ACEScg");
        assert_eq!(acescg.gamut, Some(ColorGamut::ACEScg));
        assert_eq!(acescg.transfer_function, Some(TransferFunction::Linear));

        let pq_hdr = config
            .find_colorspace("Rec.2020 - PQ")
            .expect("HDR10 should resolve");
        assert_eq!(pq_hdr.gamut, Some(ColorGamut::Rec2020));
        assert_eq!(pq_hdr.transfer_function, Some(TransferFunction::PQ));
    }

    #[test]
    fn test_parse_custom_yaml_ocio_config() {
        let yaml = r#"
ocio_profile_version: 2
search_path: "custom_luts"

roles:
  reference: ACES2065-1
  scene_linear: ACEScg
  color_timing: ACEScct
  default: sRGB

displays:
  sRGB:
    - !<View> {name: Standard, colorspace: sRGB}
    - !<View> {name: Filmic, colorspace: Rec.709}
  Rec.2020:
    - !<View> {name: HDR10, colorspace: Rec.2020 - PQ}

colorspaces:
  - !<ColorSpace>
    name: ACES2065-1
    family: ACES
    bitdepth: 32f
    isdata: false
    description: "ACES AP0 Archival"

  - !<ColorSpace>
    name: ACEScg
    family: ACES
    bitdepth: 32f
    isdata: false
    description: "ACES AP1 Linear Working Space"

  - !<ColorSpace>
    name: ACEScct
    family: ACES
    bitdepth: 32f
    isdata: false
    description: "ACES Log with Toe"

  - !<ColorSpace>
    name: sRGB
    family: Displays
    bitdepth: 8i
    isdata: false

  - !<ColorSpace>
    name: Rec.709
    family: Displays
    bitdepth: 8i
    isdata: false

  - !<ColorSpace>
    name: Rec.2020 - PQ
    family: HDR
    bitdepth: 10i
    isdata: false
"#;

        let parsed = OcioConfig::from_yaml_str(yaml).expect("YAML should parse cleanly");
        assert_eq!(parsed.ocio_profile_version, 2);
        assert_eq!(parsed.search_path.as_deref(), Some("custom_luts"));
        assert_eq!(
            parsed.roles.get("color_timing").map(|s| s.as_str()),
            Some("ACEScct")
        );
        assert_eq!(parsed.colorspaces.len(), 6);

        // Verify resolution
        let dst_cs = parsed
            .resolve_display_view("Rec.2020", "HDR10")
            .expect("View should resolve");
        assert_eq!(dst_cs.name, "Rec.2020 - PQ");
        assert_eq!(dst_cs.gamut, Some(ColorGamut::Rec2020));
        assert_eq!(dst_cs.transfer_function, Some(TransferFunction::PQ));
    }

    #[test]
    fn test_resolve_pipeline_acescg_to_sdr_and_hdr() {
        let config = OcioConfig::aces_default();

        // 1. ACEScg to sRGB SDR (should automatically trigger filmic tone mapping)
        let sdr_pipe = config
            .resolve_pipeline("scene_linear", "sRGB", "Standard")
            .expect("SDR pipeline should resolve");
        assert_eq!(sdr_pipe.input_gamut, ColorGamut::ACEScg);
        assert_eq!(sdr_pipe.input_tf, TransferFunction::Linear);
        assert_eq!(sdr_pipe.output_gamut, ColorGamut::Rec709);
        assert_eq!(sdr_pipe.output_tf, TransferFunction::SRGB);
        assert!(sdr_pipe.apply_tone_mapping);

        // 2. ACEScg to Rec.2020 HDR10 PQ (no tone mapping needed; wide gamut retained)
        let hdr_pipe = config
            .resolve_pipeline("scene_linear", "Rec.2020", "HDR10 - 1000 nits")
            .expect("HDR pipeline should resolve");
        assert_eq!(hdr_pipe.input_gamut, ColorGamut::ACEScg);
        assert_eq!(hdr_pipe.input_tf, TransferFunction::Linear);
        assert_eq!(hdr_pipe.output_gamut, ColorGamut::Rec2020);
        assert_eq!(hdr_pipe.output_tf, TransferFunction::PQ);
        assert!(!hdr_pipe.apply_tone_mapping);
    }
}
