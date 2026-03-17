use std::collections::HashMap;

use atlas_core::geo_utils::haversine_distance;
use atlas_core::telemetry::{SegmentSpeed, TripTelemetry};
use atlas_core::{rfc3339_diff_secs, rfc3339_now};

use crate::graph::road_network::RoadGraph;
use crate::penalties::PenaltyOverlay;
use crate::snap::NodeSnapper;

const MIN_SAMPLES: usize = 3;
const SLOW_THRESHOLD: f64 = 0.6;
const MIN_SPEED_KMH: f64 = 1.0;
const MAX_SPEED_KMH: f64 = 200.0;

struct SegmentSamples {
    samples: Vec<f64>,
}

impl SegmentSamples {
    fn new() -> Self {
        Self {
            samples: Vec::new(),
        }
    }

    fn add(&mut self, speed_kmh: f64) {
        if (MIN_SPEED_KMH..=MAX_SPEED_KMH).contains(&speed_kmh) {
            self.samples.push(speed_kmh);
        }
    }
}

pub struct SpeedMap {
    segments: HashMap<(u32, u32), SegmentSamples>,
}

impl SpeedMap {
    pub fn new() -> Self {
        Self {
            segments: HashMap::new(),
        }
    }

    pub fn from_segment_speeds(speeds: &[SegmentSpeed]) -> Self {
        let mut map = Self::new();
        for seg in speeds {
            let entry = map
                .segments
                .entry((seg.source_node, seg.target_node))
                .or_insert_with(SegmentSamples::new);
            for _ in 0..seg.sample_count {
                entry.add(seg.avg_speed_kmh);
            }
        }
        map
    }

    pub fn ingest_trip(&mut self, trip: &TripTelemetry, snapper: &NodeSnapper, graph: &RoadGraph) {
        if trip.waypoints.len() < 2 {
            return;
        }

        let snapped: Vec<Option<u32>> = trip
            .waypoints
            .iter()
            .map(|wp| snapper.snap(wp.lat, wp.lon))
            .collect();

        for window_idx in 0..trip.waypoints.len() - 1 {
            let (src_node, tgt_node) = match (snapped[window_idx], snapped[window_idx + 1]) {
                (Some(s), Some(t)) => (s, t),
                _ => continue,
            };

            if src_node == tgt_node {
                continue;
            }

            if !is_valid_edge(graph, src_node, tgt_node) {
                continue;
            }

            let wp_a = &trip.waypoints[window_idx];
            let wp_b = &trip.waypoints[window_idx + 1];

            let dist_m = haversine_distance(wp_a.lat, wp_a.lon, wp_b.lat, wp_b.lon);
            if dist_m < 10.0 {
                continue;
            }

            let time_s = rfc3339_diff_secs(&wp_a.timestamp, &wp_b.timestamp).unwrap_or(-1.0);
            if time_s <= 0.0 {
                continue;
            }

            let speed_kmh = (dist_m / time_s) * 3.6;

            if !(MIN_SPEED_KMH..=MAX_SPEED_KMH).contains(&speed_kmh) {
                continue;
            }

            let entry = self
                .segments
                .entry((src_node, tgt_node))
                .or_insert_with(SegmentSamples::new);
            entry.add(speed_kmh);
        }
    }

    pub fn compute_segment_speeds(&self) -> Vec<SegmentSpeed> {
        let now = rfc3339_now();
        let mut result = Vec::new();

        for (&(src, tgt), samples) in &self.segments {
            if samples.samples.len() < MIN_SAMPLES {
                continue;
            }

            let mut sorted = samples.samples.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let count = sorted.len();
            let avg = sorted.iter().sum::<f64>() / count as f64;
            let median = if count % 2 == 0 {
                (sorted[count / 2 - 1] + sorted[count / 2]) / 2.0
            } else {
                sorted[count / 2]
            };
            let p85_idx = ((count as f64) * 0.85).ceil() as usize;
            let p85 = sorted[p85_idx.min(count - 1)];

            result.push(SegmentSpeed {
                source_node: src,
                target_node: tgt,
                sample_count: count as u32,
                avg_speed_kmh: avg,
                median_speed_kmh: median,
                p85_speed_kmh: p85,
                last_updated: now.clone(),
            });
        }

        result
    }

    pub fn to_penalty_overlay(&self, profile_speeds: &HashMap<(u32, u32), f64>) -> PenaltyOverlay {
        let mut penalties: HashMap<(u32, u32), f64> = HashMap::new();

        for (&(src, tgt), samples) in &self.segments {
            if samples.samples.len() < MIN_SAMPLES {
                continue;
            }

            let avg_speed: f64 = samples.samples.iter().sum::<f64>() / samples.samples.len() as f64;

            let expected = match profile_speeds.get(&(src, tgt)) {
                Some(&speed) if speed > 0.0 => speed,
                _ => continue,
            };

            if avg_speed < expected * SLOW_THRESHOLD {
                let penalty = expected / avg_speed;
                penalties.insert((src, tgt), penalty.min(5.0));
            }
        }

        PenaltyOverlay::from_map(penalties)
    }

    pub fn num_segments(&self) -> usize {
        self.segments.len()
    }

    pub fn num_segments_with_enough_data(&self) -> usize {
        self.segments
            .values()
            .filter(|s| s.samples.len() >= MIN_SAMPLES)
            .count()
    }
}

impl Default for SpeedMap {
    fn default() -> Self {
        Self::new()
    }
}

fn is_valid_edge(graph: &RoadGraph, source: u32, target: u32) -> bool {
    for edge in graph.edges_of(source) {
        if edge.target == target {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::edge::{make_flags, Access, Edge, RoadClass, Surface};
    use crate::graph::road_network::RoadGraph;
    use crate::snap::NodeSnapper;
    use atlas_core::telemetry::{TelemetryPoint, TripTelemetry};

    fn test_flags() -> u16 {
        make_flags(
            RoadClass::Primary,
            Surface::Paved,
            false,
            false,
            Access::Yes,
            Access::Yes,
            false,
        )
    }

    fn make_two_node_graph() -> RoadGraph {
        let flags = test_flags();
        RoadGraph {
            first_edge: vec![0, 1, 2],
            edges: vec![
                Edge {
                    target: 1,
                    geo_index: 0,
                    shortcut_mid: 0,
                    distance_m: 500,
                    time_ds: 225,
                    flags,
                    _padding: 0,
                },
                Edge {
                    target: 0,
                    geo_index: 1,
                    shortcut_mid: 0,
                    distance_m: 500,
                    time_ds: 225,
                    flags,
                    _padding: 0,
                },
            ],
            node_lat: vec![5.600, 5.601],
            node_lon: vec![-0.200, -0.201],
        }
    }

    fn make_trip_on_edge(num_points: usize, speed_mps: f64) -> TripTelemetry {
        let start_lat = 5.6001;
        let start_lon = -0.2001;
        let end_lat = 5.6009;
        let end_lon = -0.2009;

        let mut waypoints = Vec::new();
        for i in 0..num_points {
            let frac = i as f64 / (num_points - 1).max(1) as f64;
            let lat = start_lat + (end_lat - start_lat) * frac;
            let lon = start_lon + (end_lon - start_lon) * frac;

            let dist_so_far = haversine_distance(start_lat, start_lon, lat, lon);
            let time_secs = if speed_mps > 0.0 {
                dist_so_far / speed_mps
            } else {
                i as f64 * 5.0
            };

            let timestamp = format!("2026-03-17T10:00:{:02}Z", time_secs.min(59.0) as u32);

            waypoints.push(TelemetryPoint {
                lat,
                lon,
                timestamp,
                speed_kmh: Some(speed_mps * 3.6),
                bearing: None,
            });
        }

        TripTelemetry {
            trip_id: "test-trip".to_string(),
            profile: "car".to_string(),
            started_at: "2026-03-17T10:00:00Z".to_string(),
            ended_at: Some("2026-03-17T10:01:00Z".to_string()),
            waypoints,
        }
    }

    #[test]
    fn ingest_trip_with_known_speeds() {
        let graph = make_two_node_graph();
        let snapper = NodeSnapper::build(&graph);
        let mut speed_map = SpeedMap::new();

        for _ in 0..5 {
            let trip = make_trip_on_edge(6, 10.0);
            speed_map.ingest_trip(&trip, &snapper, &graph);
        }

        let speeds = speed_map.compute_segment_speeds();
        assert!(
            !speeds.is_empty(),
            "should have computed some segment speeds"
        );

        for seg in &speeds {
            assert!(seg.avg_speed_kmh > 0.0);
            assert!(seg.median_speed_kmh > 0.0);
            assert!(seg.p85_speed_kmh >= seg.median_speed_kmh);
            assert!(seg.sample_count >= MIN_SAMPLES as u32);
        }
    }

    #[test]
    fn compute_segment_speeds_requires_min_samples() {
        let graph = make_two_node_graph();
        let snapper = NodeSnapper::build(&graph);
        let mut speed_map = SpeedMap::new();

        let trip = make_trip_on_edge(4, 10.0);
        speed_map.ingest_trip(&trip, &snapper, &graph);

        let speeds = speed_map.compute_segment_speeds();
        for seg in &speeds {
            assert!(seg.sample_count >= MIN_SAMPLES as u32);
        }
    }

    #[test]
    fn to_penalty_overlay_produces_correct_multipliers() {
        let mut speed_map = SpeedMap::new();

        let key = (0u32, 1u32);
        let entry = speed_map
            .segments
            .entry(key)
            .or_insert_with(SegmentSamples::new);
        for _ in 0..10 {
            entry.add(30.0);
        }

        let mut profile_speeds = HashMap::new();
        profile_speeds.insert((0u32, 1u32), 60.0);

        let overlay = speed_map.to_penalty_overlay(&profile_speeds);
        let penalty = overlay.get_penalty(0, 1);
        assert!(
            (penalty - 2.0).abs() < 0.01,
            "expected penalty ~2.0, got {}",
            penalty
        );
    }

    #[test]
    fn to_penalty_overlay_no_penalty_when_speed_close() {
        let mut speed_map = SpeedMap::new();

        let key = (0u32, 1u32);
        let entry = speed_map
            .segments
            .entry(key)
            .or_insert_with(SegmentSamples::new);
        for _ in 0..10 {
            entry.add(55.0);
        }

        let mut profile_speeds = HashMap::new();
        profile_speeds.insert((0u32, 1u32), 60.0);

        let overlay = speed_map.to_penalty_overlay(&profile_speeds);
        let penalty = overlay.get_penalty(0, 1);
        assert!(
            (penalty - 1.0).abs() < 0.01,
            "expected no penalty (1.0), got {}",
            penalty
        );
    }

    #[test]
    fn from_segment_speeds_restores_data() {
        let speeds = vec![SegmentSpeed {
            source_node: 0,
            target_node: 1,
            sample_count: 10,
            avg_speed_kmh: 45.0,
            median_speed_kmh: 42.0,
            p85_speed_kmh: 55.0,
            last_updated: "2026-03-17T10:00:00Z".to_string(),
        }];

        let speed_map = SpeedMap::from_segment_speeds(&speeds);
        assert_eq!(speed_map.num_segments(), 1);
        assert_eq!(speed_map.num_segments_with_enough_data(), 1);
    }

    #[test]
    fn parse_iso8601_basic() {
        let secs = atlas_core::rfc3339_timestamp_secs("2026-03-17T10:00:00Z");
        assert!(secs.is_some());

        let secs2 = atlas_core::rfc3339_timestamp_secs("2026-03-17T10:00:30Z");
        assert!(secs2.is_some());
        assert!((secs2.unwrap() - secs.unwrap() - 30.0).abs() < 0.01);
    }

    #[test]
    fn timestamp_diff() {
        let diff =
            atlas_core::rfc3339_diff_secs("2026-03-17T10:00:00Z", "2026-03-17T10:00:30Z").unwrap();
        assert!((diff - 30.0).abs() < 0.01);
    }

    #[test]
    fn timestamp_diff_accepts_offsets() {
        let diff =
            atlas_core::rfc3339_diff_secs("2026-03-17T10:00:00+01:00", "2026-03-17T10:00:30+01:00")
                .unwrap();
        assert!((diff - 30.0).abs() < 0.01);
    }

    #[test]
    fn empty_speed_map() {
        let speed_map = SpeedMap::new();
        assert_eq!(speed_map.num_segments(), 0);
        assert_eq!(speed_map.num_segments_with_enough_data(), 0);
        assert!(speed_map.compute_segment_speeds().is_empty());
    }

    #[test]
    fn penalty_capped_at_5x() {
        let mut speed_map = SpeedMap::new();

        let key = (0u32, 1u32);
        let entry = speed_map
            .segments
            .entry(key)
            .or_insert_with(SegmentSamples::new);
        for _ in 0..10 {
            entry.add(5.0);
        }

        let mut profile_speeds = HashMap::new();
        profile_speeds.insert((0u32, 1u32), 120.0);

        let overlay = speed_map.to_penalty_overlay(&profile_speeds);
        let penalty = overlay.get_penalty(0, 1);
        assert!(
            penalty <= 5.0,
            "penalty should be capped at 5.0, got {}",
            penalty
        );
    }
}
