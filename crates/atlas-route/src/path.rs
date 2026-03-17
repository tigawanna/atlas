use crate::ch::preprocess::ChGraph;
use crate::ch::query::RouteResult;
use crate::graph::edge::Edge;

#[derive(Debug, Clone)]
pub struct PathSegment {
    pub points: Vec<(f64, f64)>,
    pub road_name: Option<String>,
    pub distance_m: f64,
    pub is_roundabout: bool,
}

pub fn unpack_route(ch: &ChGraph, result: &RouteResult) -> Vec<PathSegment> {
    let mut segments = Vec::new();

    for &(source_node, edge_idx) in &result.path_edges_forward {
        let edge = &ch.forward_graph.edges[edge_idx];
        unpack_edge(ch, source_node, edge.target, edge, &mut segments);
    }

    let backward_edges: Vec<(u32, usize)> =
        result.path_edges_backward.iter().rev().cloned().collect();
    for &(source_node, edge_idx) in &backward_edges {
        let edge = &ch.backward_graph.edges[edge_idx];
        unpack_backward_edge(ch, source_node, edge.target, edge, &mut segments);
    }

    segments
}

fn unpack_edge(
    ch: &ChGraph,
    source: u32,
    target: u32,
    edge: &Edge,
    segments: &mut Vec<PathSegment>,
) {
    if let Some(mid) = edge.decoded_shortcut_mid() {
        if let Some((first_edge_data, first_target)) =
            find_edge_in_graph(&ch.forward_graph, source, mid)
        {
            unpack_edge(ch, source, first_target, first_edge_data, segments);
        }

        if let Some((second_edge_data, second_target)) =
            find_edge_in_graph(&ch.forward_graph, mid, target)
        {
            unpack_edge(ch, mid, second_target, second_edge_data, segments);
        }
    } else {
        let geo_idx = edge.geo_index;
        if (geo_idx as usize) < ch.geometry.first_point.len().saturating_sub(1) {
            let (lats, lons) = ch.geometry.get_points(geo_idx);
            let points: Vec<(f64, f64)> = lats
                .iter()
                .zip(lons.iter())
                .map(|(&lat, &lon)| (lat as f64, lon as f64))
                .collect();
            let road_name = ch.geometry.get_road_name(geo_idx).map(|s| s.to_string());
            segments.push(PathSegment {
                points,
                road_name,
                distance_m: edge.distance_m as f64,
                is_roundabout: edge.is_roundabout(),
            });
        } else {
            let (slat, slon) = ch.forward_graph.node_coords(source);
            let (tlat, tlon) = ch.forward_graph.node_coords(target);
            segments.push(PathSegment {
                points: vec![(slat as f64, slon as f64), (tlat as f64, tlon as f64)],
                road_name: None,
                distance_m: edge.distance_m as f64,
                is_roundabout: edge.is_roundabout(),
            });
        }
    }
}

fn unpack_backward_edge(
    ch: &ChGraph,
    source: u32,
    target: u32,
    edge: &Edge,
    segments: &mut Vec<PathSegment>,
) {
    if let Some(mid) = edge.decoded_shortcut_mid() {
        if let Some((second_edge_data, second_target)) =
            find_edge_in_graph(&ch.backward_graph, mid, source)
        {
            unpack_backward_edge(ch, mid, second_target, second_edge_data, segments);
        }

        if let Some((first_edge_data, first_target)) =
            find_edge_in_graph(&ch.backward_graph, target, mid)
        {
            unpack_backward_edge(ch, target, first_target, first_edge_data, segments);
        }
    } else {
        let geo_idx = edge.geo_index;
        if (geo_idx as usize) < ch.geometry.first_point.len().saturating_sub(1) {
            let (lats, lons) = ch.geometry.get_points(geo_idx);
            let points: Vec<(f64, f64)> = lats
                .iter()
                .zip(lons.iter())
                .map(|(&lat, &lon)| (lat as f64, lon as f64))
                .collect();
            let mut reversed_points = points;
            reversed_points.reverse();
            let road_name = ch.geometry.get_road_name(geo_idx).map(|s| s.to_string());
            segments.push(PathSegment {
                points: reversed_points,
                road_name,
                distance_m: edge.distance_m as f64,
                is_roundabout: edge.is_roundabout(),
            });
        } else {
            let (slat, slon) = ch.backward_graph.node_coords(target);
            let (tlat, tlon) = ch.backward_graph.node_coords(source);
            segments.push(PathSegment {
                points: vec![(slat as f64, slon as f64), (tlat as f64, tlon as f64)],
                road_name: None,
                distance_m: edge.distance_m as f64,
                is_roundabout: edge.is_roundabout(),
            });
        }
    }
}

fn find_edge_in_graph(
    graph: &crate::graph::RoadGraph,
    source: u32,
    target: u32,
) -> Option<(&Edge, u32)> {
    for edge in graph.edges_of(source) {
        if edge.target == target {
            return Some((edge, target));
        }
    }
    None
}

pub fn segments_to_polyline(segments: &[PathSegment]) -> Vec<(f64, f64)> {
    let mut polyline = Vec::new();
    for segment in segments {
        for &point in &segment.points {
            if polyline.last() != Some(&point) {
                polyline.push(point);
            }
        }
    }
    polyline
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ch::preprocess::build_ch;
    use crate::ch::query::ChQuery;
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
            road_names: vec![
                Some("Main St".to_string()),
                Some("Highway 1".to_string()),
                Some("Main St".to_string()),
                Some("Side Rd".to_string()),
                Some("Highway 1".to_string()),
                Some("Side Rd".to_string()),
            ],
        };

        (graph, geometry)
    }

    #[test]
    fn unpack_produces_segments() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let result = query.route(&ch, 0, 2);
        if let Some(result) = result {
            let segments = unpack_route(&ch, &result);
            assert!(!segments.is_empty());
            for seg in &segments {
                assert!(!seg.points.is_empty());
            }
        }
    }

    #[test]
    fn unpack_route_preserves_roundabout_flags_from_ch_edges() {
        let flags = make_flags(
            RoadClass::Primary,
            Surface::Paved,
            false,
            false,
            Access::Yes,
            Access::Yes,
            true,
        );
        let graph = RoadGraph {
            first_edge: vec![0, 1, 2],
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
                    target: 0,
                    geo_index: 1,
                    shortcut_mid: 0,
                    distance_m: 100,
                    time_ds: 50,
                    flags,
                    _padding: 0,
                },
            ],
            node_lat: vec![5.0, 5.001],
            node_lon: vec![-0.1, -0.101],
        };
        let geometry = RoadGeometry {
            first_point: vec![0, 2, 4],
            coords_lat: vec![5.0, 5.001, 5.001, 5.0],
            coords_lon: vec![-0.1, -0.101, -0.101, -0.1],
            road_names: vec![Some("Roundabout".to_string()); 2],
        };
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let result = query.route(&ch, 0, 1).unwrap();
        let segments = unpack_route(&ch, &result);

        assert_eq!(segments.len(), 1);
        assert!(segments[0].is_roundabout);
    }

    #[test]
    fn polyline_is_continuous() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let result = query.route(&ch, 0, 2);
        if let Some(result) = result {
            let segments = unpack_route(&ch, &result);
            let polyline = segments_to_polyline(&segments);
            assert!(polyline.len() >= 2);
            for pair in polyline.windows(2) {
                assert_ne!(pair[0], pair[1]);
            }
        }
    }

    #[test]
    fn empty_route_produces_no_segments() {
        let result = RouteResult {
            distance_m: 0,
            duration_ds: 0,
            path_edges_forward: Vec::new(),
            path_edges_backward: Vec::new(),
            meeting_node: 0,
        };
        let ch = ChGraph {
            forward_graph: RoadGraph::default(),
            backward_graph: RoadGraph::default(),
            geometry: RoadGeometry::default(),
            ch_level: Vec::new(),
            num_original_nodes: 0,
            profile_name: "test".to_string(),
        };
        let segments = unpack_route(&ch, &result);
        assert!(segments.is_empty());
    }
}
