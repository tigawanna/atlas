use crate::error::AtlasError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileFormat {
    Mvt,
    Png,
    Webp,
    Json,
}

impl TileFormat {
    pub fn from_extension(ext: &str) -> Result<Self, AtlasError> {
        match ext {
            "mvt" | "pbf" => Ok(Self::Mvt),
            "png" => Ok(Self::Png),
            "webp" => Ok(Self::Webp),
            "json" => Ok(Self::Json),
            other => Err(AtlasError::InvalidFormat(format!(
                "unsupported tile format: {other}"
            ))),
        }
    }

    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Mvt => "application/vnd.mapbox-vector-tile",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Json => "application/json",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mvt_extensions() {
        assert_eq!(TileFormat::from_extension("mvt").unwrap(), TileFormat::Mvt);
        assert_eq!(TileFormat::from_extension("pbf").unwrap(), TileFormat::Mvt);
    }

    #[test]
    fn parses_image_formats() {
        assert_eq!(TileFormat::from_extension("png").unwrap(), TileFormat::Png);
        assert_eq!(
            TileFormat::from_extension("webp").unwrap(),
            TileFormat::Webp
        );
    }

    #[test]
    fn rejects_unknown() {
        assert!(TileFormat::from_extension("gif").is_err());
    }

    #[test]
    fn correct_content_types() {
        assert_eq!(
            TileFormat::Mvt.content_type(),
            "application/vnd.mapbox-vector-tile"
        );
        assert_eq!(TileFormat::Png.content_type(), "image/png");
    }
}
