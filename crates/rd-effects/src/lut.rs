//! 3D Look-Up Table (.cube) Parser.
//!
//! Section W2.2 of the Technical Proposal:
//! Parses industry-standard .cube LUT files for cinematic color grading.

use anyhow::{Context, Result, anyhow};

/// A parsed 3D Look-Up Table.
#[derive(Debug, Clone)]
pub struct Lut3D {
    pub title: String,
    pub size: usize,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    /// Flat RGBA floating point values in 3D order (size^3 * 4 floats).
    pub data_rgba: Vec<f32>,
}

impl Lut3D {
    /// Parse a .cube format file from a string.
    pub fn parse_cube(cube_str: &str) -> Result<Self> {
        let mut title = String::from("LUT");
        let mut size = 0usize;
        let mut domain_min = [0.0, 0.0, 0.0];
        let mut domain_max = [1.0, 1.0, 1.0];
        let mut data_rgba = Vec::new();

        for line in cube_str.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with("TITLE") {
                if let Some(t) = line.strip_prefix("TITLE") {
                    title = t.trim().trim_matches('"').to_string();
                }
            } else if line.starts_with("LUT_3D_SIZE") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    size = parts[1].parse().context("Invalid LUT_3D_SIZE")?;
                }
            } else if line.starts_with("DOMAIN_MIN") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    domain_min = [parts[1].parse()?, parts[2].parse()?, parts[3].parse()?];
                }
            } else if line.starts_with("DOMAIN_MAX") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    domain_max = [parts[1].parse()?, parts[2].parse()?, parts[3].parse()?];
                }
            } else {
                // Must be RGB float values
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let r: f32 = parts[0].parse()?;
                    let g: f32 = parts[1].parse()?;
                    let b: f32 = parts[2].parse()?;
                    data_rgba.push(r);
                    data_rgba.push(g);
                    data_rgba.push(b);
                    data_rgba.push(1.0); // Alpha
                }
            }
        }

        if size == 0 {
            return Err(anyhow!("Missing LUT_3D_SIZE specification"));
        }

        let expected_samples = size * size * size * 4;
        if data_rgba.len() != expected_samples {
            return Err(anyhow!(
                "LUT sample count mismatch: expected {} values, got {}",
                expected_samples,
                data_rgba.len()
            ));
        }

        Ok(Self {
            title,
            size,
            domain_min,
            domain_max,
            data_rgba,
        })
    }

    /// Generate a neutral identity 3D LUT (output = input).
    pub fn identity(size: usize) -> Self {
        let mut data_rgba = Vec::with_capacity(size * size * size * 4);
        let step = 1.0 / (size as f32 - 1.0).max(1.0);

        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    data_rgba.push(r as f32 * step);
                    data_rgba.push(g as f32 * step);
                    data_rgba.push(b as f32 * step);
                    data_rgba.push(1.0);
                }
            }
        }

        Self {
            title: "Identity".into(),
            size,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
            data_rgba,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cube_format() {
        let sample = r#"
# Sample 2x2x2 cube LUT
TITLE "TealOrange"
LUT_3D_SIZE 2
0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
"#;
        let lut = Lut3D::parse_cube(sample).expect("Parsed cube");
        assert_eq!(lut.title, "TealOrange");
        assert_eq!(lut.size, 2);
        assert_eq!(lut.data_rgba.len(), 2 * 2 * 2 * 4);
    }

    #[test]
    fn test_identity_lut() {
        let lut = Lut3D::identity(4);
        assert_eq!(lut.size, 4);
        assert_eq!(lut.data_rgba.len(), 4 * 4 * 4 * 4);
        // First entry is 0,0,0,1
        assert_eq!(&lut.data_rgba[0..4], &[0.0, 0.0, 0.0, 1.0]);
        // Last entry is 1,1,1,1
        let last_idx = lut.data_rgba.len() - 4;
        assert_eq!(&lut.data_rgba[last_idx..], &[1.0, 1.0, 1.0, 1.0]);
    }
}
