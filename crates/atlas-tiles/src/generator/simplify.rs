use geo::Simplify;
use geo_types::LineString;

pub fn tolerance_for_zoom(zoom: u8) -> f64 {
    match zoom {
        0..=2 => 0.01,
        3..=5 => 0.005,
        6..=8 => 0.001,
        9..=11 => 0.0005,
        12..=14 => 0.0001,
        _ => 0.00005,
    }
}

pub fn simplify_line(coords: &[(f64, f64)], zoom: u8) -> Vec<(f64, f64)> {
    if coords.len() < 3 {
        return coords.to_vec();
    }

    let tolerance = tolerance_for_zoom(zoom);
    let line: LineString<f64> = coords.iter().copied().collect();
    let simplified = line.simplify(&tolerance);

    simplified
        .into_inner()
        .into_iter()
        .map(|c| (c.x, c.y))
        .collect()
}

pub fn tile_for_point(lat: f64, lon: f64, zoom: u8) -> (u32, u32) {
    let n = (1u64 << zoom) as f64;
    let x = ((lon + 180.0) / 360.0 * n).floor() as u32;
    let lat_rad = lat.to_radians();
    let y = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n)
        .floor() as u32;

    let max = 1u32 << zoom;
    (x.min(max - 1), y.min(max - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerance_decreases_with_zoom() {
        assert!(tolerance_for_zoom(0) > tolerance_for_zoom(5));
        assert!(tolerance_for_zoom(5) > tolerance_for_zoom(10));
        assert!(tolerance_for_zoom(10) > tolerance_for_zoom(14));
    }

    #[test]
    fn tolerance_known_values() {
        assert_eq!(tolerance_for_zoom(0), 0.01);
        assert_eq!(tolerance_for_zoom(6), 0.001);
        assert_eq!(tolerance_for_zoom(14), 0.0001);
    }

    #[test]
    fn simplify_reduces_point_count() {
        let coords: Vec<(f64, f64)> = (0..100)
            .map(|i| {
                let t = i as f64 * 0.01;
                (t, (t * 10.0).sin() * 0.0001 + t)
            })
            .collect();

        let simplified = simplify_line(&coords, 2);
        assert!(
            simplified.len() < coords.len(),
            "simplified {} should be less than original {}",
            simplified.len(),
            coords.len()
        );
        assert!(simplified.len() >= 2);
    }

    #[test]
    fn simplify_preserves_short_lines() {
        let coords = vec![(0.0, 0.0), (1.0, 1.0)];
        let result = simplify_line(&coords, 10);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn tile_for_accra_z10() {
        let (x, y) = tile_for_point(5.603, -0.187, 10);
        assert_eq!(x, 511);
        assert_eq!(y, 496);
    }

    #[test]
    fn tile_for_origin_z0() {
        let (x, y) = tile_for_point(0.0, 0.0, 0);
        assert_eq!(x, 0);
        assert_eq!(y, 0);
    }

    #[test]
    fn tile_for_point_clamps_to_bounds() {
        let (x, y) = tile_for_point(85.0, 179.99, 1);
        assert!(x < 2);
        assert!(y < 2);
    }
}
