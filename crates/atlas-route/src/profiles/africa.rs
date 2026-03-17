use crate::graph::{Edge, Surface};

pub fn apply_africa_modifier(base_speed_kmh: f64, edge: &Edge) -> Option<f64> {
    if edge.is_seasonal_closure() {
        return None;
    }
    let modifier = match edge.surface() {
        Surface::Paved => 1.0,
        Surface::Unpaved => 0.5,
        Surface::Track => 0.3,
        Surface::Unknown => 0.8,
    };
    Some(base_speed_kmh * modifier)
}

pub fn speed_to_deciseconds(distance_m: u32, speed_kmh: f64) -> u32 {
    if speed_kmh <= 0.0 {
        return u32::MAX;
    }
    let time_s = (distance_m as f64) / (speed_kmh * 1000.0 / 3600.0);
    (time_s * 10.0) as u32
}
