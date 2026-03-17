use serde::Serialize;

use crate::ch::preprocess::ChGraph;
use crate::ch::query::ChQuery;
use crate::dijkstra::dijkstra_astar;
use crate::graph::RoadGraph;
use crate::penalties::PenaltyOverlay;
use crate::profiles::RoutingProfile;
use crate::snap::NodeSnapper;

#[derive(Debug, Clone, Serialize)]
pub struct MatrixResult {
    pub distances_m: Vec<Vec<Option<u32>>>,
    pub durations_s: Vec<Vec<Option<u32>>>,
}

pub fn compute_matrix(
    ch: &ChGraph,
    snapper: &NodeSnapper,
    query: &mut ChQuery,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
) -> MatrixResult {
    let origin_nodes: Vec<Option<u32>> = origins
        .iter()
        .map(|&(lat, lon)| snapper.snap(lat, lon))
        .collect();

    let dest_nodes: Vec<Option<u32>> = destinations
        .iter()
        .map(|&(lat, lon)| snapper.snap(lat, lon))
        .collect();

    let mut distances_m = vec![vec![None; destinations.len()]; origins.len()];
    let mut durations_s = vec![vec![None; destinations.len()]; origins.len()];

    for (i, src_opt) in origin_nodes.iter().enumerate() {
        let src = match src_opt {
            Some(s) => *s,
            None => continue,
        };

        for (j, dst_opt) in dest_nodes.iter().enumerate() {
            let dst = match dst_opt {
                Some(d) => *d,
                None => continue,
            };

            if let Some(result) = query.route(ch, src, dst) {
                distances_m[i][j] = Some(result.distance_m);
                durations_s[i][j] = Some(result.duration_ds / 10);
            }
        }
    }

    MatrixResult {
        distances_m,
        durations_s,
    }
}

pub fn compute_matrix_dijkstra(
    graph: &RoadGraph,
    profile: &dyn RoutingProfile,
    penalties: Option<&PenaltyOverlay>,
    origin_nodes: &[Option<u32>],
    dest_nodes: &[Option<u32>],
) -> MatrixResult {
    let mut distances_m = vec![vec![None; dest_nodes.len()]; origin_nodes.len()];
    let mut durations_s = vec![vec![None; dest_nodes.len()]; origin_nodes.len()];

    for (i, src_opt) in origin_nodes.iter().enumerate() {
        let src = match src_opt {
            Some(node) => *node,
            None => continue,
        };

        for (j, dst_opt) in dest_nodes.iter().enumerate() {
            let dst = match dst_opt {
                Some(node) => *node,
                None => continue,
            };

            if let Some(result) = dijkstra_astar(graph, profile, src, dst, penalties) {
                distances_m[i][j] = Some(result.distance_m);
                durations_s[i][j] = Some(result.duration_ds / 10);
            }
        }
    }

    MatrixResult {
        distances_m,
        durations_s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ch::preprocess::build_ch;
    use crate::graph::edge::{make_flags, Access, Edge, RoadClass, Surface};
    use crate::graph::road_network::{RoadGeometry, RoadGraph};
    use crate::profiles::CarProfile;

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

    fn build_triangle_graph() -> (RoadGraph, RoadGeometry) {
        let flags = test_flags();

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
    fn matrix_2x2() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let snapper = NodeSnapper::build(&graph);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let origins = vec![(5.0, -0.1), (5.1, -0.2)];
        let destinations = vec![(5.1, -0.2), (5.2, -0.3)];

        let result = compute_matrix(&ch, &snapper, &mut query, &origins, &destinations);

        assert_eq!(result.distances_m.len(), 2);
        assert_eq!(result.distances_m[0].len(), 2);
        assert_eq!(result.durations_s.len(), 2);
        assert_eq!(result.durations_s[0].len(), 2);

        assert!(result.distances_m[0][0].is_some());
    }

    #[test]
    fn matrix_unroutable_returns_none() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let snapper = NodeSnapper::build(&graph);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let origins = vec![(5.0, -0.1)];
        let destinations = vec![(50.0, 50.0)];

        let result = compute_matrix(&ch, &snapper, &mut query, &origins, &destinations);

        assert_eq!(result.distances_m[0][0], None);
        assert_eq!(result.durations_s[0][0], None);
    }

    #[test]
    fn matrix_same_origin_destination() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let snapper = NodeSnapper::build(&graph);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let origins = vec![(5.0, -0.1)];
        let destinations = vec![(5.0, -0.1)];

        let result = compute_matrix(&ch, &snapper, &mut query, &origins, &destinations);

        assert_eq!(result.distances_m[0][0], Some(0));
        assert_eq!(result.durations_s[0][0], Some(0));
    }
}
