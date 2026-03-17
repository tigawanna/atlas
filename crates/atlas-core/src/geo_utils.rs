const EARTH_RADIUS_M: f64 = 6_371_000.0;

pub fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let lat1_r = lat1.to_radians();
    let lat2_r = lat2.to_radians();
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2) + lat1_r.cos() * lat2_r.cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_RADIUS_M * c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accra_to_kumasi() {
        let dist = haversine_distance(5.603, -0.187, 6.687, -1.624);
        assert!(
            dist > 180_000.0 && dist < 280_000.0,
            "expected ~200-240km, got {dist}"
        );
    }

    #[test]
    fn same_point_is_zero() {
        let dist = haversine_distance(5.0, -0.1, 5.0, -0.1);
        assert!(dist < 1.0);
    }
}
