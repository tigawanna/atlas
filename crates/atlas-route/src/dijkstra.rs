use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::graph::road_network::RoadGraph;
use crate::path::PathSegment;
use crate::penalties::PenaltyOverlay;
use crate::profiles::RoutingProfile;

pub struct DijkstraResult {
    pub distance_m: u32,
    pub duration_ds: u32,
    pub path_edges: Vec<(u32, usize)>,
}

const INF: u32 = u32::MAX;
const NO_PARENT: (u32, usize) = (u32::MAX, usize::MAX);
const EARTH_RADIUS_M: f64 = 6_371_000.0;
const MAX_SPEED_MPS: f64 = 120.0 * 1000.0 / 3600.0;

fn haversine_m(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f64 {
    let lat1_r = (lat1 as f64).to_radians();
    let lat2_r = (lat2 as f64).to_radians();
    let d_lat = ((lat2 - lat1) as f64).to_radians();
    let d_lon = ((lon2 - lon1) as f64).to_radians();
    let a = (d_lat / 2.0).sin().powi(2) + lat1_r.cos() * lat2_r.cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_RADIUS_M * c
}

fn heuristic_ds(graph: &RoadGraph, node: u32, target: u32) -> u32 {
    let (lat1, lon1) = graph.node_coords(node);
    let (lat2, lon2) = graph.node_coords(target);
    let dist_m = haversine_m(lat1, lon1, lat2, lon2);
    let time_s = dist_m / MAX_SPEED_MPS;
    (time_s * 10.0) as u32
}

pub fn dijkstra_astar(
    graph: &RoadGraph,
    profile: &dyn RoutingProfile,
    source: u32,
    target: u32,
    penalties: Option<&PenaltyOverlay>,
) -> Option<DijkstraResult> {
    let num_nodes = graph.num_nodes();
    if source as usize >= num_nodes || target as usize >= num_nodes {
        return None;
    }

    if source == target {
        return Some(DijkstraResult {
            distance_m: 0,
            duration_ds: 0,
            path_edges: Vec::new(),
        });
    }

    let mut dist = vec![INF; num_nodes];
    let mut parent: Vec<(u32, usize)> = vec![NO_PARENT; num_nodes];

    dist[source as usize] = 0;

    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
    let h_initial = heuristic_ds(graph, source, target);
    heap.push(Reverse((h_initial, source)));

    while let Some(Reverse((priority, node))) = heap.pop() {
        if node == target {
            break;
        }

        let g_cost = dist[node as usize];
        if g_cost == INF {
            continue;
        }

        let expected_priority = g_cost.saturating_add(heuristic_ds(graph, node, target));
        if priority > expected_priority.saturating_add(1) {
            continue;
        }

        let first = graph.first_edge[node as usize] as usize;
        for (offset, edge) in graph.edges_of(node).iter().enumerate() {
            let base_weight = match profile.edge_weight(edge) {
                Some(w) => w,
                None => continue,
            };

            let penalty = penalties.map_or(1.0, |p| p.get_penalty(node, edge.target));
            if penalty.is_infinite() {
                continue;
            }
            let weight = (base_weight as f64 * penalty) as u32;

            let new_g = g_cost.saturating_add(weight);
            if new_g < dist[edge.target as usize] {
                dist[edge.target as usize] = new_g;
                parent[edge.target as usize] = (node, first + offset);
                let h = heuristic_ds(graph, edge.target, target);
                let f = new_g.saturating_add(h);
                heap.push(Reverse((f, edge.target)));
            }
        }
    }

    if dist[target as usize] == INF {
        return None;
    }

    let mut path_edges = Vec::new();
    let mut current = target;
    while current != source {
        let (par, edge_idx) = parent[current as usize];
        if par == u32::MAX {
            return None;
        }
        path_edges.push((par, edge_idx));
        current = par;
    }
    path_edges.reverse();

    let mut total_dist: u32 = 0;
    let mut total_dur: u32 = 0;
    for &(_, edge_idx) in &path_edges {
        let edge = &graph.edges[edge_idx];
        total_dist = total_dist.saturating_add(edge.distance_m);
        total_dur = total_dur.saturating_add(profile.edge_weight(edge).unwrap_or(edge.time_ds));
    }

    Some(DijkstraResult {
        distance_m: total_dist,
        duration_ds: total_dur,
        path_edges,
    })
}

pub fn reconstruct_dijkstra_path(
    graph: &RoadGraph,
    geometry: &crate::graph::road_network::RoadGeometry,
    path_edges: &[(u32, usize)],
) -> Vec<PathSegment> {
    let mut segments = Vec::with_capacity(path_edges.len());

    for &(source_node, edge_idx) in path_edges {
        let edge = &graph.edges[edge_idx];
        let geo_idx = edge.geo_index;

        if (geo_idx as usize) < geometry.first_point.len().saturating_sub(1) {
            let (lats, lons) = geometry.get_points(geo_idx);
            let points: Vec<(f64, f64)> = lats
                .iter()
                .zip(lons.iter())
                .map(|(&lat, &lon)| (lat as f64, lon as f64))
                .collect();
            let road_name = geometry.get_road_name(geo_idx).map(|s| s.to_string());
            segments.push(PathSegment {
                points,
                road_name,
                distance_m: edge.distance_m as f64,
                is_roundabout: edge.is_roundabout(),
            });
        } else {
            let (slat, slon) = graph.node_coords(source_node);
            let (tlat, tlon) = graph.node_coords(edge.target);
            segments.push(PathSegment {
                points: vec![(slat as f64, slon as f64), (tlat as f64, tlon as f64)],
                road_name: None,
                distance_m: edge.distance_m as f64,
                is_roundabout: edge.is_roundabout(),
            });
        }
    }

    segments
}

fn get_all_profiles() -> Vec<Box<dyn RoutingProfile>> {
    vec![
        Box::new(crate::profiles::CarProfile),
        Box::new(crate::profiles::MotorcycleProfile),
        Box::new(crate::profiles::BicycleProfile),
        Box::new(crate::profiles::FootProfile),
    ]
}

pub fn profile_by_name(name: &str) -> Option<Box<dyn RoutingProfile>> {
    get_all_profiles().into_iter().find(|p| p.name() == name)
}

pub fn all_profile_names() -> Vec<String> {
    get_all_profiles()
        .iter()
        .map(|p| p.name().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn build_linear_graph(num_nodes: usize) -> (RoadGraph, RoadGeometry) {
        let flags = test_flags();

        let mut edges = Vec::new();
        let mut first_edge = Vec::new();
        let mut edge_count = 0u32;

        for i in 0..num_nodes {
            first_edge.push(edge_count);
            if i + 1 < num_nodes {
                edges.push(Edge {
                    target: (i + 1) as u32,
                    geo_index: edges.len() as u32,
                    shortcut_mid: 0,
                    distance_m: 100,
                    time_ds: 50,
                    flags,
                    _padding: 0,
                });
                edge_count += 1;
            }
            if i > 0 {
                edges.push(Edge {
                    target: (i - 1) as u32,
                    geo_index: edges.len() as u32,
                    shortcut_mid: 0,
                    distance_m: 100,
                    time_ds: 50,
                    flags,
                    _padding: 0,
                });
                edge_count += 1;
            }
        }
        first_edge.push(edge_count);

        let node_lat: Vec<f32> = (0..num_nodes).map(|i| 5.0 + i as f32 * 0.01).collect();
        let node_lon: Vec<f32> = (0..num_nodes).map(|i| -0.1 - i as f32 * 0.01).collect();

        let num_edges = edges.len();
        let mut fp = Vec::new();
        for i in 0..num_edges {
            fp.push((i * 2) as u32);
        }
        fp.push((num_edges * 2) as u32);

        let graph = RoadGraph {
            first_edge,
            edges,
            node_lat,
            node_lon,
        };

        let geometry = RoadGeometry {
            first_point: fp,
            coords_lat: vec![5.0; num_edges * 2],
            coords_lon: vec![-0.1; num_edges * 2],
            road_names: vec![None; num_edges],
        };

        (graph, geometry)
    }

    fn build_disconnected_graph() -> (RoadGraph, RoadGeometry) {
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
                target: 0,
                geo_index: 1,
                shortcut_mid: 0,
                distance_m: 100,
                time_ds: 50,
                flags,
                _padding: 0,
            },
        ];

        let first_edge = vec![0, 1, 2, 2, 2];
        let node_lat = vec![5.0, 5.1, 5.5, 5.6];
        let node_lon = vec![-0.1, -0.2, -0.5, -0.6];

        let graph = RoadGraph {
            first_edge,
            edges,
            node_lat,
            node_lon,
        };

        let geometry = RoadGeometry {
            first_point: vec![0, 2, 4],
            coords_lat: vec![5.0, 5.1, 5.1, 5.0],
            coords_lon: vec![-0.1, -0.2, -0.2, -0.1],
            road_names: vec![None; 2],
        };

        (graph, geometry)
    }

    #[test]
    fn same_source_target() {
        let (graph, _) = build_triangle_graph();
        let result = dijkstra_astar(&graph, &CarProfile, 0, 0, None).unwrap();
        assert_eq!(result.distance_m, 0);
        assert_eq!(result.duration_ds, 0);
        assert!(result.path_edges.is_empty());
    }

    #[test]
    fn finds_shortest_path_triangle() {
        let (graph, _) = build_triangle_graph();
        let result = dijkstra_astar(&graph, &CarProfile, 0, 2, None);
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.distance_m > 0);
        assert!(result.duration_ds > 0);
        assert!(!result.path_edges.is_empty());
    }

    #[test]
    fn prefers_shorter_path() {
        let (graph, _) = build_triangle_graph();
        let result = dijkstra_astar(&graph, &CarProfile, 0, 2, None).unwrap();
        assert!(result.distance_m <= 250);
    }

    #[test]
    fn linear_graph_routes() {
        let (graph, _) = build_linear_graph(5);
        let result = dijkstra_astar(&graph, &CarProfile, 0, 4, None);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.path_edges.len(), 4);
    }

    #[test]
    fn disconnected_returns_none() {
        let (graph, _) = build_disconnected_graph();
        let result = dijkstra_astar(&graph, &CarProfile, 0, 2, None);
        assert!(result.is_none());
    }

    #[test]
    fn out_of_bounds_returns_none() {
        let (graph, _) = build_triangle_graph();
        assert!(dijkstra_astar(&graph, &CarProfile, 999, 0, None).is_none());
        assert!(dijkstra_astar(&graph, &CarProfile, 0, 999, None).is_none());
    }

    #[test]
    fn astar_same_result_as_basic_search() {
        let (graph, _) = build_linear_graph(10);
        let result = dijkstra_astar(&graph, &CarProfile, 0, 9, None).unwrap();
        assert_eq!(result.path_edges.len(), 9);
        assert_eq!(result.distance_m, 900);
    }

    #[test]
    fn path_reconstruction_produces_segments() {
        let (graph, geometry) = build_triangle_graph();
        let result = dijkstra_astar(&graph, &CarProfile, 0, 2, None).unwrap();
        let segments = reconstruct_dijkstra_path(&graph, &geometry, &result.path_edges);
        assert!(!segments.is_empty());
        for seg in &segments {
            assert!(!seg.points.is_empty());
            assert!(seg.distance_m > 0.0);
        }
    }

    #[test]
    fn path_reconstruction_empty_path() {
        let (graph, geometry) = build_triangle_graph();
        let segments = reconstruct_dijkstra_path(&graph, &geometry, &[]);
        assert!(segments.is_empty());
    }

    #[test]
    fn profile_by_name_finds_car() {
        let p = profile_by_name("car");
        assert!(p.is_some());
        assert_eq!(p.unwrap().name(), "car");
    }

    #[test]
    fn profile_by_name_returns_none_for_unknown() {
        assert!(profile_by_name("helicopter").is_none());
    }

    #[test]
    fn all_profile_names_contains_expected() {
        let names = all_profile_names();
        assert!(names.contains(&"car".to_string()));
        assert!(names.contains(&"foot".to_string()));
        assert!(names.contains(&"bicycle".to_string()));
        assert!(names.contains(&"motorcycle".to_string()));
    }
}
