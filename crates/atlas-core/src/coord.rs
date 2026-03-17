use crate::error::AtlasError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl TileCoord {
    pub fn new(z: u8, x: u32, y: u32) -> Result<Self, AtlasError> {
        if z > 22 {
            return Err(AtlasError::InvalidCoord(format!(
                "zoom {z} exceeds maximum 22"
            )));
        }
        let max = 1u32 << z;
        if x >= max || y >= max {
            return Err(AtlasError::InvalidCoord(format!(
                "x={x}, y={y} out of bounds for zoom {z} (max {max})"
            )));
        }
        Ok(Self { z, x, y })
    }
}

impl From<TileCoord> for pmtiles::TileCoord {
    fn from(c: TileCoord) -> Self {
        pmtiles::TileCoord::new(c.z, c.x, c.y).expect("TileCoord already validated")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_coords() {
        assert!(TileCoord::new(0, 0, 0).is_ok());
        assert!(TileCoord::new(1, 1, 1).is_ok());
        assert!(TileCoord::new(22, 0, 0).is_ok());
    }

    #[test]
    fn zoom_too_high() {
        assert!(TileCoord::new(23, 0, 0).is_err());
    }

    #[test]
    fn x_out_of_bounds() {
        assert!(TileCoord::new(0, 1, 0).is_err());
        assert!(TileCoord::new(1, 2, 0).is_err());
    }

    #[test]
    fn y_out_of_bounds() {
        assert!(TileCoord::new(0, 0, 1).is_err());
    }

    #[test]
    fn converts_to_pmtiles_coord() {
        let coord = TileCoord::new(5, 10, 15).unwrap();
        let pmt: pmtiles::TileCoord = coord.into();
        assert_eq!(pmt.z(), 5);
        assert_eq!(pmt.x(), 10);
        assert_eq!(pmt.y(), 15);
    }
}
