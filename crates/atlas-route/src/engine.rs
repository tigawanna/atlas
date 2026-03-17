use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

use atlas_core::{AtlasError, ContributionStore, TelemetryStore, TripTelemetry};

use crate::ch::preprocess::ChGraph;
use crate::ch::query::ChQuery;
use crate::ch::serialize::load_ch;
use crate::dijkstra::{
    all_profile_names, dijkstra_astar, profile_by_name, reconstruct_dijkstra_path,
};
use crate::graph::builder::build_road_graph;
use crate::graph::road_network::{RoadGeometry, RoadGraph};
use crate::instructions::{generate_instructions, Instruction};
use crate::matrix::{compute_matrix, compute_matrix_dijkstra, MatrixResult};
use crate::path::{segments_to_polyline, unpack_route};
use crate::penalties::PenaltyOverlay;
use crate::snap::NodeSnapper;
use crate::speed_data::SpeedMap;

#[derive(Debug, Clone, Serialize)]
pub struct GeoJsonLineString {
    #[serde(rename = "type")]
    pub geom_type: String,
    pub coordinates: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FullRouteResult {
    pub distance_m: u32,
    pub duration_s: u32,
    pub geometry: GeoJsonLineString,
    pub instructions: Vec<Instruction>,
}

pub struct RouteEngine {
    profiles: HashMap<String, Arc<ChGraph>>,
    road_graph: Option<Arc<RoadGraph>>,
    road_geometry: Option<Arc<RoadGeometry>>,
    snapper: NodeSnapper,
    query_pool: Vec<Arc<Mutex<ChQuery>>>,
    penalties: Arc<PenaltyOverlay>,
    speed_map: Arc<Mutex<SpeedMap>>,
}

impl RouteEngine {
    pub fn load(
        route_dir: &Path,
        osm_dir: Option<&Path>,
        contributions_dir: Option<&Path>,
    ) -> Result<Self, AtlasError> {
        Self::load_with_speed_data(route_dir, osm_dir, contributions_dir, None)
    }

    pub fn load_with_speed_data(
        route_dir: &Path,
        osm_dir: Option<&Path>,
        contributions_dir: Option<&Path>,
        speed_data_path: Option<&Path>,
    ) -> Result<Self, AtlasError> {
        let mut profiles: HashMap<String, Arc<ChGraph>> = HashMap::new();

        if let Ok(entries) = std::fs::read_dir(route_dir) {
            for entry in entries {
                let entry = entry.map_err(|e| AtlasError::StoreError(e.to_string()))?;
                let path = entry.path();
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();

                if !file_name.starts_with("ch-") || !file_name.ends_with(".bin") {
                    continue;
                }

                let ch = load_ch(&path)?;
                let profile_name = ch.profile_name.clone();
                tracing::info!(profile = %profile_name, path = %path.display(), "loaded CH graph");
                profiles.insert(profile_name, Arc::new(ch));
            }
        }

        let speed_map = Self::load_speed_map(speed_data_path);

        if !profiles.is_empty() {
            let first_graph = profiles.values().next().unwrap();
            let snapper = NodeSnapper::build(&first_graph.forward_graph);

            let pool_size = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                * 2;

            let num_nodes = first_graph.forward_graph.num_nodes();
            let query_pool = (0..pool_size)
                .map(|_| Arc::new(Mutex::new(ChQuery::new(num_nodes))))
                .collect();

            let penalties = Self::build_penalties(contributions_dir, &snapper)?;

            return Ok(Self {
                profiles,
                road_graph: None,
                road_geometry: None,
                snapper,
                query_pool,
                penalties: Arc::new(penalties),
                speed_map: Arc::new(Mutex::new(speed_map)),
            });
        }

        let osm_path = osm_dir.ok_or_else(|| {
            AtlasError::StoreError(
                "no CH graphs found and no OSM directory configured for on-demand routing"
                    .to_string(),
            )
        })?;

        if !osm_path.exists() {
            return Err(AtlasError::StoreError(format!(
                "OSM directory does not exist: {}",
                osm_path.display()
            )));
        }

        tracing::info!(osm_dir = %osm_path.display(), "no CH graphs found, building road graph from OSM for on-demand routing");
        let (graph, geometry) = build_road_graph(osm_path)?;
        tracing::info!(
            nodes = graph.num_nodes(),
            edges = graph.num_edges(),
            "road graph built for on-demand Dijkstra routing"
        );

        let snapper = NodeSnapper::build(&graph);
        let penalties = Self::build_penalties(contributions_dir, &snapper)?;

        Ok(Self {
            profiles: HashMap::new(),
            road_graph: Some(Arc::new(graph)),
            road_geometry: Some(Arc::new(geometry)),
            snapper,
            query_pool: Vec::new(),
            penalties: Arc::new(penalties),
            speed_map: Arc::new(Mutex::new(speed_map)),
        })
    }

    pub async fn route(
        &self,
        origin: (f64, f64),
        dest: (f64, f64),
        profile: &str,
    ) -> Result<FullRouteResult, AtlasError> {
        let origin_node = self
            .snapper
            .snap(origin.0, origin.1)
            .ok_or_else(|| AtlasError::StoreError("origin coordinate too far from road".into()))?;

        let dest_node = self.snapper.snap(dest.0, dest.1).ok_or_else(|| {
            AtlasError::StoreError("destination coordinate too far from road".into())
        })?;

        if let Some(ch) = self.profiles.get(profile) {
            let query_result = self.acquire_and_run(origin_node, dest_node, ch).await?;

            let segments = unpack_route(ch, &query_result);
            let polyline = segments_to_polyline(&segments);
            let instructions = generate_instructions(&segments);
            let coordinates: Vec<[f64; 2]> =
                polyline.iter().map(|&(lat, lon)| [lon, lat]).collect();

            return Ok(FullRouteResult {
                distance_m: query_result.distance_m,
                duration_s: query_result.duration_ds / 10,
                geometry: GeoJsonLineString {
                    geom_type: "LineString".to_string(),
                    coordinates,
                },
                instructions,
            });
        }

        if let (Some(graph), Some(geometry)) = (&self.road_graph, &self.road_geometry) {
            let routing_profile = profile_by_name(profile)
                .ok_or_else(|| AtlasError::StoreError(format!("unknown profile: {profile}")))?;

            let graph_arc = Arc::clone(graph);
            let penalties_arc = Arc::clone(&self.penalties);

            let result = tokio::task::spawn_blocking(move || {
                dijkstra_astar(
                    &graph_arc,
                    routing_profile.as_ref(),
                    origin_node,
                    dest_node,
                    Some(&penalties_arc),
                )
            })
            .await
            .map_err(|e| AtlasError::StoreError(format!("dijkstra task failed: {e}")))?
            .ok_or_else(|| AtlasError::StoreError("no route found".into()))?;

            let segments = reconstruct_dijkstra_path(graph, geometry, &result.path_edges);
            let polyline = segments_to_polyline(&segments);
            let instructions = generate_instructions(&segments);
            let coordinates: Vec<[f64; 2]> =
                polyline.iter().map(|&(lat, lon)| [lon, lat]).collect();

            return Ok(FullRouteResult {
                distance_m: result.distance_m,
                duration_s: result.duration_ds / 10,
                geometry: GeoJsonLineString {
                    geom_type: "LineString".to_string(),
                    coordinates,
                },
                instructions,
            });
        }

        Err(AtlasError::StoreError(format!(
            "no routing available for profile: {profile}"
        )))
    }

    pub async fn matrix(
        &self,
        origins: &[(f64, f64)],
        dests: &[(f64, f64)],
        profile: &str,
    ) -> Result<MatrixResult, AtlasError> {
        if let Ok(ch) = self.get_ch_graph(profile) {
            let snapper = NodeSnapper::build(&ch.forward_graph);
            let origins_vec: Vec<(f64, f64)> = origins.to_vec();
            let dests_vec: Vec<(f64, f64)> = dests.to_vec();

            let result = tokio::task::spawn_blocking(move || {
                let mut query = ChQuery::new(ch.forward_graph.num_nodes());
                compute_matrix(&ch, &snapper, &mut query, &origins_vec, &dests_vec)
            })
            .await
            .map_err(|e| AtlasError::StoreError(format!("matrix task failed: {e}")))?;

            return Ok(result);
        }

        if let Some(graph) = &self.road_graph {
            let routing_profile = profile_by_name(profile)
                .ok_or_else(|| AtlasError::StoreError(format!("unknown profile: {profile}")))?;
            let origin_nodes: Vec<Option<u32>> = origins
                .iter()
                .map(|&(lat, lon)| self.snapper.snap(lat, lon))
                .collect();
            let dest_nodes: Vec<Option<u32>> = dests
                .iter()
                .map(|&(lat, lon)| self.snapper.snap(lat, lon))
                .collect();
            let graph_arc = Arc::clone(graph);
            let penalties_arc = Arc::clone(&self.penalties);

            let result = tokio::task::spawn_blocking(move || {
                compute_matrix_dijkstra(
                    &graph_arc,
                    routing_profile.as_ref(),
                    Some(&penalties_arc),
                    &origin_nodes,
                    &dest_nodes,
                )
            })
            .await
            .map_err(|e| AtlasError::StoreError(format!("matrix task failed: {e}")))?;

            return Ok(result);
        }

        Err(AtlasError::StoreError(format!(
            "no routing available for profile: {profile}"
        )))
    }

    pub fn available_profiles(&self) -> Vec<String> {
        if !self.profiles.is_empty() {
            let mut names: Vec<String> = self.profiles.keys().cloned().collect();
            names.sort();
            return names;
        }

        if self.road_graph.is_some() {
            let mut names = all_profile_names();
            names.sort();
            return names;
        }

        Vec::new()
    }

    fn get_ch_graph(&self, profile: &str) -> Result<Arc<ChGraph>, AtlasError> {
        self.profiles
            .get(profile)
            .cloned()
            .ok_or_else(|| AtlasError::StoreError(format!("unknown CH profile: {profile}")))
    }

    pub fn reload_penalties(&mut self, store: &ContributionStore) -> Result<(), AtlasError> {
        let overlay = PenaltyOverlay::from_contributions(store, &self.snapper)?;
        tracing::info!(
            penalties = overlay.num_penalties(),
            "reloaded penalty overlay from contributions"
        );
        self.penalties = Arc::new(overlay);
        Ok(())
    }

    pub async fn ingest_trip(
        &self,
        trip: &TripTelemetry,
        speed_data_path: Option<&Path>,
    ) -> Result<(), AtlasError> {
        if let Some(graph) = &self.road_graph {
            let mut speed_map = self.speed_map.lock().await;
            speed_map.ingest_trip(trip, &self.snapper, graph);

            if let Some(path) = speed_data_path {
                let speeds = speed_map.compute_segment_speeds();
                TelemetryStore::save_segment_speeds(&speeds, path)?;
                tracing::info!(segments = speeds.len(), "saved updated segment speeds");
            }
        }
        Ok(())
    }

    pub fn snapper(&self) -> &NodeSnapper {
        &self.snapper
    }

    pub fn road_graph(&self) -> Option<&Arc<RoadGraph>> {
        self.road_graph.as_ref()
    }

    fn build_penalties(
        contributions_dir: Option<&Path>,
        snapper: &NodeSnapper,
    ) -> Result<PenaltyOverlay, AtlasError> {
        let Some(dir) = contributions_dir else {
            return Ok(PenaltyOverlay::empty());
        };

        let store = ContributionStore::new(dir);
        let overlay = PenaltyOverlay::from_contributions(&store, snapper)?;
        tracing::info!(
            penalties = overlay.num_penalties(),
            "loaded penalty overlay from contributions"
        );
        Ok(overlay)
    }

    fn load_speed_map(speed_data_path: Option<&Path>) -> SpeedMap {
        let Some(path) = speed_data_path else {
            return SpeedMap::new();
        };

        match TelemetryStore::load_segment_speeds(path) {
            Ok(speeds) if !speeds.is_empty() => {
                tracing::info!(
                    segments = speeds.len(),
                    path = %path.display(),
                    "loaded speed data from disk"
                );
                SpeedMap::from_segment_speeds(&speeds)
            }
            Ok(_) => SpeedMap::new(),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to load speed data, starting with empty speed map"
                );
                SpeedMap::new()
            }
        }
    }

    async fn acquire_and_run(
        &self,
        source: u32,
        target: u32,
        ch: &Arc<ChGraph>,
    ) -> Result<crate::ch::query::RouteResult, AtlasError> {
        for slot in &self.query_pool {
            if let Ok(slot_guard) = slot.try_lock() {
                drop(slot_guard);
                let slot_arc = Arc::clone(slot);
                let ch_arc = Arc::clone(ch);
                return tokio::task::spawn_blocking(move || {
                    let mut query = slot_arc.blocking_lock();
                    query
                        .route(&ch_arc, source, target)
                        .ok_or_else(|| AtlasError::StoreError("no route found".into()))
                })
                .await
                .map_err(|e| AtlasError::StoreError(format!("route task failed: {e}")))?;
            }
        }

        let ch_arc = Arc::clone(ch);
        let num_nodes = ch.forward_graph.num_nodes();
        tokio::task::spawn_blocking(move || {
            let mut query = ChQuery::new(num_nodes);
            query
                .route(&ch_arc, source, target)
                .ok_or_else(|| AtlasError::StoreError("no route found".into()))
        })
        .await
        .map_err(|e| AtlasError::StoreError(format!("route task failed: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::edge::{make_flags, Access, Edge, RoadClass, Surface};
    use crate::graph::road_network::{RoadGeometry, RoadGraph};

    fn test_graph() -> (RoadGraph, RoadGeometry) {
        let flags = make_flags(
            RoadClass::Primary,
            Surface::Paved,
            false,
            false,
            Access::Yes,
            Access::Yes,
            false,
        );
        let graph = RoadGraph {
            first_edge: vec![0, 2, 3, 3],
            edges: vec![
                Edge {
                    target: 1,
                    geo_index: 0,
                    shortcut_mid: 0,
                    distance_m: 100,
                    time_ds: 50,
                    flags,
                    _padding: 0,
                },
                Edge {
                    target: 2,
                    geo_index: 1,
                    shortcut_mid: 0,
                    distance_m: 300,
                    time_ds: 150,
                    flags,
                    _padding: 0,
                },
                Edge {
                    target: 2,
                    geo_index: 2,
                    shortcut_mid: 0,
                    distance_m: 100,
                    time_ds: 50,
                    flags,
                    _padding: 0,
                },
            ],
            node_lat: vec![5.0, 5.001, 5.002],
            node_lon: vec![-0.1, -0.101, -0.102],
        };
        let geometry = RoadGeometry {
            first_point: vec![0, 2, 4, 6],
            coords_lat: vec![5.0, 5.001, 5.0, 5.002, 5.001, 5.002],
            coords_lon: vec![-0.1, -0.101, -0.1, -0.102, -0.101, -0.102],
            road_names: vec![None; 3],
        };
        (graph, geometry)
    }

    #[tokio::test]
    async fn matrix_falls_back_to_dijkstra_without_ch_graphs() {
        let (graph, geometry) = test_graph();
        let snapper = NodeSnapper::build(&graph);
        let engine = RouteEngine {
            profiles: HashMap::new(),
            road_graph: Some(Arc::new(graph)),
            road_geometry: Some(Arc::new(geometry)),
            snapper,
            query_pool: Vec::new(),
            penalties: Arc::new(PenaltyOverlay::empty()),
            speed_map: Arc::new(Mutex::new(SpeedMap::new())),
        };

        let result = engine
            .matrix(&[(5.0, -0.1)], &[(5.002, -0.102)], "car")
            .await
            .unwrap();

        assert_eq!(result.distances_m[0][0], Some(200));
        assert_eq!(result.durations_s[0][0], Some(9));
    }
}
