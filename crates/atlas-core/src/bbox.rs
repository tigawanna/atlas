use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
}

impl BBox {
    pub fn new(min_lon: f64, min_lat: f64, max_lon: f64, max_lat: f64) -> Self {
        debug_assert!(min_lon < max_lon, "BBox min_lon must be less than max_lon");
        debug_assert!(min_lat < max_lat, "BBox min_lat must be less than max_lat");
        Self {
            min_lon,
            min_lat,
            max_lon,
            max_lat,
        }
    }

    pub fn try_new(min_lon: f64, min_lat: f64, max_lon: f64, max_lat: f64) -> Option<Self> {
        if min_lon < max_lon && min_lat < max_lat {
            Some(Self {
                min_lon,
                min_lat,
                max_lon,
                max_lat,
            })
        } else {
            None
        }
    }

    pub fn contains(&self, lon: f64, lat: f64) -> bool {
        lon >= self.min_lon && lon <= self.max_lon && lat >= self.min_lat && lat <= self.max_lat
    }
}

pub const AFRICA: BBox = BBox {
    min_lon: -25.0,
    min_lat: -35.0,
    max_lon: 55.0,
    max_lat: 38.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accra_is_in_africa() {
        assert!(AFRICA.contains(-0.187, 5.603));
    }

    #[test]
    fn london_is_not_in_africa() {
        assert!(!AFRICA.contains(-0.118, 51.509));
    }
}
