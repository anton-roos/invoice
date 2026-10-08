//! The company logo. `Company::logo` is a `data:image/...;base64,` URL of
//! the uploaded image (or a path to an image file), so the logo lives in the
//! database rather than in the app.

use base64::Engine as _;

/// What the original web app stored for its bundled logo. That file no
/// longer ships, so this value means "no logo" until one is chosen.
const LEGACY_BUNDLED: &str = "assets/logo.png";

/// Decoded RGBA pixels, ready for both the preview and the PDF.
pub struct LogoImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// The original file, so a PDF can embed a JPEG or PNG as-is.
    pub source: Vec<u8>,
}

/// `None` when there is no logo; `Err` when it can't be decoded.
pub fn load(logo: &str) -> Result<Option<LogoImage>, String> {
    let bytes = if logo.is_empty() || logo == LEGACY_BUNDLED {
        return Ok(None);
    } else if let Some(rest) = logo.strip_prefix("data:") {
        let (_, b64) = rest.split_once(";base64,").ok_or("logo is not a base64 data URL")?;
        base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("logo data is not valid base64: {e}"))?
    } else {
        std::fs::read(logo).map_err(|e| format!("could not read logo {logo}: {e}"))?
    };
    let img = image::load_from_memory(&bytes).map_err(|e| format!("could not decode logo: {e}"))?;
    let rgba = img.to_rgba8();
    Ok(Some(LogoImage { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw(), source: bytes }))
}

/// Turn an image file into the `data:` URL stored in the database.
pub fn to_data_url(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let format = image::guess_format(&bytes).map_err(|_| "that file is not a supported image")?;
    image::load_from_memory(&bytes).map_err(|e| format!("could not decode image: {e}"))?;
    let mime = format.to_mime_type();
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

/// A small generated logo for tests, as a `data:` URL.
#[cfg(test)]
pub fn test_logo_url() -> String {
    let img = image::RgbaImage::from_fn(120, 40, |x, _| image::Rgba([(x * 2) as u8, 40, 90, 255]));
    let dir = std::env::temp_dir().join(format!("invoice-logo-{}", crate::db::new_id("")));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("logo.png");
    img.save(&path).unwrap();
    let url = to_data_url(&path).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    url
}

#[cfg(test)]
mod tests {
    #[test]
    fn data_url_logo_round_trips() {
        let url = super::test_logo_url();
        assert!(url.starts_with("data:image/png;base64,"));
        let logo = super::load(&url).unwrap().unwrap();
        assert_eq!((logo.width, logo.height), (120, 40));
    }

    #[test]
    fn legacy_bundled_logo_means_none() {
        assert!(super::load("assets/logo.png").unwrap().is_none());
        assert!(super::load("").unwrap().is_none());
    }
}
