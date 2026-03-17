use serde::Serialize;

use crate::place::Lang;

#[derive(Debug, Clone)]
pub struct ParsedQuery {
    pub tokens: Vec<String>,
    pub lang: Option<Lang>,
    pub locality: Option<Locality>,
    pub landmark_ref: Option<LandmarkRef>,
    pub street: Option<String>,
    pub descriptors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Locality {
    pub neighborhood: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LandmarkRef {
    pub name: String,
    pub relation: SpatialRelation,
}

#[derive(Debug, Clone)]
pub enum SpatialRelation {
    Near,
    Behind,
    Past,
    Beside,
    Between(String),
    Opposite,
}

#[derive(Debug, Clone)]
pub struct GeocodeOpts {
    pub limit: usize,
    pub country: Option<String>,
    pub lang: Option<Lang>,
}

impl Default for GeocodeOpts {
    fn default() -> Self {
        Self {
            limit: 5,
            country: None,
            lang: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReverseOpts {
    pub limit: usize,
    pub lang: Option<Lang>,
    pub radius_m: f64,
}

impl Default for ReverseOpts {
    fn default() -> Self {
        Self {
            limit: 5,
            lang: None,
            radius_m: 500.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GeocodeResult {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub category: String,
    pub address: Option<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReverseResult {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub distance_m: f64,
    pub category: String,
}
