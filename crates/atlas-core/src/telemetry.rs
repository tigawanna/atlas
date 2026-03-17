use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::AtlasError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripTelemetry {
    pub trip_id: String,
    pub profile: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub waypoints: Vec<TelemetryPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryPoint {
    pub lat: f64,
    pub lon: f64,
    pub timestamp: String,
    pub speed_kmh: Option<f64>,
    pub bearing: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentSpeed {
    pub source_node: u32,
    pub target_node: u32,
    pub sample_count: u32,
    pub avg_speed_kmh: f64,
    pub median_speed_kmh: f64,
    pub p85_speed_kmh: f64,
    pub last_updated: String,
}

pub struct TelemetryStore {
    dir: PathBuf,
}

impl TelemetryStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    pub fn save_trip(&self, trip: &TripTelemetry) -> Result<(), AtlasError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| {
            AtlasError::StoreError(format!(
                "failed to create telemetry dir {}: {}",
                self.dir.display(),
                e
            ))
        })?;

        let file_path = self.dir.join(format!("{}.json", trip.trip_id));
        let json = serde_json::to_string_pretty(trip)
            .map_err(|e| AtlasError::StoreError(format!("failed to serialize trip: {}", e)))?;

        std::fs::write(&file_path, json).map_err(|e| {
            AtlasError::StoreError(format!(
                "failed to write trip file {}: {}",
                file_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    pub fn load_trip(&self, trip_id: &str) -> Result<Option<TripTelemetry>, AtlasError> {
        let file_path = self.dir.join(format!("{}.json", trip_id));
        if !file_path.exists() {
            return Ok(None);
        }

        let contents = std::fs::read_to_string(&file_path).map_err(|e| {
            AtlasError::StoreError(format!(
                "failed to read trip file {}: {}",
                file_path.display(),
                e
            ))
        })?;

        let trip: TripTelemetry = serde_json::from_str(&contents).map_err(|e| {
            AtlasError::StoreError(format!(
                "failed to parse trip file {}: {}",
                file_path.display(),
                e
            ))
        })?;

        Ok(Some(trip))
    }

    pub fn list_trips(&self) -> Result<Vec<TripTelemetry>, AtlasError> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }

        let entries = std::fs::read_dir(&self.dir).map_err(|e| {
            AtlasError::StoreError(format!(
                "failed to read telemetry dir {}: {}",
                self.dir.display(),
                e
            ))
        })?;

        let mut trips = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| AtlasError::StoreError(e.to_string()))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let contents = std::fs::read_to_string(&path).map_err(|e| {
                AtlasError::StoreError(format!(
                    "failed to read trip file {}: {}",
                    path.display(),
                    e
                ))
            })?;
            let trip: TripTelemetry = serde_json::from_str(&contents).map_err(|e| {
                AtlasError::StoreError(format!(
                    "failed to parse trip file {}: {}",
                    path.display(),
                    e
                ))
            })?;
            trips.push(trip);
        }

        Ok(trips)
    }

    pub fn save_segment_speeds(speeds: &[SegmentSpeed], path: &Path) -> Result<(), AtlasError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AtlasError::StoreError(format!(
                    "failed to create parent dir for {}: {}",
                    path.display(),
                    e
                ))
            })?;
        }

        let encoded = bincode::serialize(speeds).map_err(|e| {
            AtlasError::StoreError(format!("failed to serialize segment speeds: {}", e))
        })?;

        std::fs::write(path, encoded).map_err(|e| {
            AtlasError::StoreError(format!(
                "failed to write segment speeds to {}: {}",
                path.display(),
                e
            ))
        })?;

        Ok(())
    }

    pub fn load_segment_speeds(path: &Path) -> Result<Vec<SegmentSpeed>, AtlasError> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let data = std::fs::read(path).map_err(|e| {
            AtlasError::StoreError(format!(
                "failed to read segment speeds from {}: {}",
                path.display(),
                e
            ))
        })?;

        let speeds: Vec<SegmentSpeed> = bincode::deserialize(&data).map_err(|e| {
            AtlasError::StoreError(format!("failed to deserialize segment speeds: {}", e))
        })?;

        Ok(speeds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_point_serialization_roundtrip() {
        let point = TelemetryPoint {
            lat: 5.603,
            lon: -0.187,
            timestamp: "2026-03-17T10:00:00Z".to_string(),
            speed_kmh: Some(35.2),
            bearing: Some(180.0),
        };

        let json = serde_json::to_string(&point).unwrap();
        let parsed: TelemetryPoint = serde_json::from_str(&json).unwrap();
        assert!((parsed.lat - 5.603).abs() < f64::EPSILON);
        assert!((parsed.lon - (-0.187)).abs() < f64::EPSILON);
        assert_eq!(parsed.timestamp, "2026-03-17T10:00:00Z");
        assert!((parsed.speed_kmh.unwrap() - 35.2).abs() < f64::EPSILON);
        assert!((parsed.bearing.unwrap() - 180.0).abs() < f64::EPSILON);
    }

    #[test]
    fn trip_telemetry_serialization_roundtrip() {
        let trip = TripTelemetry {
            trip_id: "test-trip-123".to_string(),
            profile: "car".to_string(),
            started_at: "2026-03-17T10:00:00Z".to_string(),
            ended_at: Some("2026-03-17T10:12:00Z".to_string()),
            waypoints: vec![
                TelemetryPoint {
                    lat: 5.603,
                    lon: -0.187,
                    timestamp: "2026-03-17T10:00:00Z".to_string(),
                    speed_kmh: Some(0.0),
                    bearing: None,
                },
                TelemetryPoint {
                    lat: 5.60,
                    lon: -0.19,
                    timestamp: "2026-03-17T10:00:05Z".to_string(),
                    speed_kmh: Some(35.2),
                    bearing: Some(180.0),
                },
            ],
        };

        let json = serde_json::to_string(&trip).unwrap();
        let parsed: TripTelemetry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.trip_id, "test-trip-123");
        assert_eq!(parsed.profile, "car");
        assert_eq!(parsed.waypoints.len(), 2);
        assert!(parsed.ended_at.is_some());
    }

    #[test]
    fn store_save_and_load_trip() {
        let dir = std::env::temp_dir().join(format!("atlas-telemetry-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let store = TelemetryStore::new(&dir);

        let trip = TripTelemetry {
            trip_id: "trip-save-test".to_string(),
            profile: "car".to_string(),
            started_at: "2026-03-17T10:00:00Z".to_string(),
            ended_at: None,
            waypoints: vec![TelemetryPoint {
                lat: 5.603,
                lon: -0.187,
                timestamp: "2026-03-17T10:00:00Z".to_string(),
                speed_kmh: None,
                bearing: None,
            }],
        };

        store.save_trip(&trip).unwrap();

        let loaded = store.load_trip("trip-save-test").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.trip_id, "trip-save-test");
        assert_eq!(loaded.waypoints.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_list_trips() {
        let dir = std::env::temp_dir().join(format!("atlas-telemetry-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let store = TelemetryStore::new(&dir);

        for i in 0..3 {
            let trip = TripTelemetry {
                trip_id: format!("trip-list-{}", i),
                profile: "car".to_string(),
                started_at: "2026-03-17T10:00:00Z".to_string(),
                ended_at: None,
                waypoints: Vec::new(),
            };
            store.save_trip(&trip).unwrap();
        }

        let trips = store.list_trips().unwrap();
        assert_eq!(trips.len(), 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_list_empty_dir() {
        let dir =
            std::env::temp_dir().join(format!("atlas-telemetry-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let store = TelemetryStore::new(&dir);
        let trips = store.list_trips().unwrap();
        assert!(trips.is_empty());
    }

    #[test]
    fn segment_speed_bincode_roundtrip() {
        let speeds = vec![
            SegmentSpeed {
                source_node: 0,
                target_node: 1,
                sample_count: 10,
                avg_speed_kmh: 45.0,
                median_speed_kmh: 42.0,
                p85_speed_kmh: 55.0,
                last_updated: "2026-03-17T10:00:00Z".to_string(),
            },
            SegmentSpeed {
                source_node: 1,
                target_node: 2,
                sample_count: 5,
                avg_speed_kmh: 30.0,
                median_speed_kmh: 28.0,
                p85_speed_kmh: 38.0,
                last_updated: "2026-03-17T10:00:00Z".to_string(),
            },
        ];

        let path =
            std::env::temp_dir().join(format!("atlas-speeds-test-{}.bin", std::process::id()));
        let _ = std::fs::remove_file(&path);

        TelemetryStore::save_segment_speeds(&speeds, &path).unwrap();
        let loaded = TelemetryStore::load_segment_speeds(&path).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].source_node, 0);
        assert_eq!(loaded[0].target_node, 1);
        assert!((loaded[0].avg_speed_kmh - 45.0).abs() < f64::EPSILON);
        assert_eq!(loaded[1].sample_count, 5);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_segment_speeds_missing_file() {
        let path = std::env::temp_dir().join("nonexistent-speeds.bin");
        let speeds = TelemetryStore::load_segment_speeds(&path).unwrap();
        assert!(speeds.is_empty());
    }

    #[test]
    fn load_trip_missing() {
        let dir = std::env::temp_dir().join(format!("atlas-telemetry-miss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let store = TelemetryStore::new(&dir);
        let result = store.load_trip("nonexistent").unwrap();
        assert!(result.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
