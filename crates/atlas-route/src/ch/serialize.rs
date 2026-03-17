use std::path::Path;

use atlas_core::AtlasError;

use super::preprocess::ChGraph;

pub fn save_ch(ch: &ChGraph, path: &Path) -> Result<(), AtlasError> {
    let bytes = bincode::serialize(ch).map_err(|e| AtlasError::StoreError(e.to_string()))?;
    std::fs::write(path, bytes).map_err(|e| AtlasError::StoreError(e.to_string()))?;
    Ok(())
}

pub fn load_ch(path: &Path) -> Result<ChGraph, AtlasError> {
    let bytes = std::fs::read(path).map_err(|e| AtlasError::StoreError(e.to_string()))?;
    bincode::deserialize(&bytes).map_err(|e| AtlasError::StoreError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ch::preprocess::build_ch;
    use crate::graph::edge::{make_flags, Access, Edge, RoadClass, Surface};
    use crate::graph::road_network::{RoadGeometry, RoadGraph};
    use crate::profiles::CarProfile;

    fn build_triangle_graph() -> (RoadGraph, RoadGeometry) {
        let flags = make_flags(
            RoadClass::Primary,
            Surface::Paved,
            false,
            false,
            Access::Yes,
            Access::Yes,
            false,
        );
        let edges = vec![
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
                distance_m: 200,
                time_ds: 100,
                flags,
                _padding: 0,
            },
            Edge {
                target: 0,
                geo_index: 2,
                shortcut_mid: 0,
                distance_m: 100,
                time_ds: 50,
                flags,
                _padding: 0,
            },
            Edge {
                target: 2,
                geo_index: 3,
                shortcut_mid: 0,
                distance_m: 150,
                time_ds: 75,
                flags,
                _padding: 0,
            },
            Edge {
                target: 0,
                geo_index: 4,
                shortcut_mid: 0,
                distance_m: 200,
                time_ds: 100,
                flags,
                _padding: 0,
            },
            Edge {
                target: 1,
                geo_index: 5,
                shortcut_mid: 0,
                distance_m: 150,
                time_ds: 75,
                flags,
                _padding: 0,
            },
        ];
        let first_edge = vec![0, 2, 4, 6];
        let node_lat = vec![5.0, 5.1, 5.2];
        let node_lon = vec![-0.1, -0.2, -0.3];

        let graph = RoadGraph {
            first_edge,
            edges,
            node_lat,
            node_lon,
        };

        let geometry = RoadGeometry {
            first_point: vec![0, 2, 4, 6, 8, 10, 12],
            coords_lat: vec![5.0, 5.1, 5.0, 5.2, 5.1, 5.0, 5.1, 5.2, 5.2, 5.0, 5.2, 5.1],
            coords_lon: vec![
                -0.1, -0.2, -0.1, -0.3, -0.2, -0.1, -0.2, -0.3, -0.3, -0.1, -0.3, -0.2,
            ],
            road_names: vec![None; 6],
        };

        (graph, geometry)
    }

    #[test]
    fn save_load_round_trip() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);

        let dir = std::env::temp_dir().join("atlas_ch_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.ch.bin");

        save_ch(&ch, &path).unwrap();
        let loaded = load_ch(&path).unwrap();

        assert_eq!(loaded.num_original_nodes, ch.num_original_nodes);
        assert_eq!(loaded.ch_level.len(), ch.ch_level.len());
        assert_eq!(loaded.profile_name, ch.profile_name);
        assert_eq!(
            loaded.forward_graph.num_nodes(),
            ch.forward_graph.num_nodes()
        );
        assert_eq!(
            loaded.backward_graph.num_nodes(),
            ch.backward_graph.num_nodes()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
