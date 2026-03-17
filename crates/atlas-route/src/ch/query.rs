use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::preprocess::ChGraph;

#[derive(Debug, Clone)]
pub struct RouteResult {
    pub distance_m: u32,
    pub duration_ds: u32,
    pub path_edges_forward: Vec<(u32, usize)>,
    pub path_edges_backward: Vec<(u32, usize)>,
    pub meeting_node: u32,
}

pub struct ChQuery {
    num_nodes: usize,
    forward_dist: Vec<u32>,
    backward_dist: Vec<u32>,
    forward_parent: Vec<(u32, usize)>,
    backward_parent: Vec<(u32, usize)>,
    touched: Vec<u32>,
}

const INF: u32 = u32::MAX;
const NO_PARENT: (u32, usize) = (u32::MAX, usize::MAX);

impl ChQuery {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            num_nodes,
            forward_dist: vec![INF; num_nodes],
            backward_dist: vec![INF; num_nodes],
            forward_parent: vec![NO_PARENT; num_nodes],
            backward_parent: vec![NO_PARENT; num_nodes],
            touched: Vec::with_capacity(1024),
        }
    }

    fn reset(&mut self) {
        for &node in &self.touched {
            let idx = node as usize;
            self.forward_dist[idx] = INF;
            self.backward_dist[idx] = INF;
            self.forward_parent[idx] = NO_PARENT;
            self.backward_parent[idx] = NO_PARENT;
        }
        self.touched.clear();
    }

    fn mark_touched(&mut self, node: u32) {
        let idx = node as usize;
        if self.forward_dist[idx] == INF && self.backward_dist[idx] == INF {
            self.touched.push(node);
        }
    }

    pub fn route(&mut self, ch: &ChGraph, source: u32, target: u32) -> Option<RouteResult> {
        if source as usize >= self.num_nodes || target as usize >= self.num_nodes {
            return None;
        }

        self.reset();

        if source == target {
            return Some(RouteResult {
                distance_m: 0,
                duration_ds: 0,
                path_edges_forward: Vec::new(),
                path_edges_backward: Vec::new(),
                meeting_node: source,
            });
        }

        self.mark_touched(source);
        self.mark_touched(target);
        self.forward_dist[source as usize] = 0;
        self.backward_dist[target as usize] = 0;

        let mut forward_heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
        let mut backward_heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();

        forward_heap.push(Reverse((0, source)));
        backward_heap.push(Reverse((0, target)));

        let mut best_distance = INF;
        let mut best_meeting = u32::MAX;

        loop {
            let fwd_min = forward_heap.peek().map(|Reverse((d, _))| *d).unwrap_or(INF);
            let bwd_min = backward_heap
                .peek()
                .map(|Reverse((d, _))| *d)
                .unwrap_or(INF);

            if fwd_min >= best_distance && bwd_min >= best_distance {
                break;
            }

            if fwd_min == INF && bwd_min == INF {
                break;
            }

            if fwd_min <= bwd_min {
                if let Some(Reverse((cost, node))) = forward_heap.pop() {
                    if cost > self.forward_dist[node as usize] {
                        continue;
                    }

                    if self.backward_dist[node as usize] != INF {
                        let total = cost.saturating_add(self.backward_dist[node as usize]);
                        if total < best_distance {
                            best_distance = total;
                            best_meeting = node;
                        }
                    }

                    for (edge_idx_offset, edge) in
                        ch.forward_graph.edges_of(node).iter().enumerate()
                    {
                        let edge_idx =
                            ch.forward_graph.first_edge[node as usize] as usize + edge_idx_offset;
                        let new_dist = cost.saturating_add(edge.time_ds);
                        if new_dist < self.forward_dist[edge.target as usize] {
                            self.mark_touched(edge.target);
                            self.forward_dist[edge.target as usize] = new_dist;
                            self.forward_parent[edge.target as usize] = (node, edge_idx);
                            forward_heap.push(Reverse((new_dist, edge.target)));
                        }
                    }
                }
            } else if let Some(Reverse((cost, node))) = backward_heap.pop() {
                if cost > self.backward_dist[node as usize] {
                    continue;
                }

                if self.forward_dist[node as usize] != INF {
                    let total = cost.saturating_add(self.forward_dist[node as usize]);
                    if total < best_distance {
                        best_distance = total;
                        best_meeting = node;
                    }
                }

                for (edge_idx_offset, edge) in ch.backward_graph.edges_of(node).iter().enumerate() {
                    let edge_idx =
                        ch.backward_graph.first_edge[node as usize] as usize + edge_idx_offset;
                    let new_dist = cost.saturating_add(edge.time_ds);
                    if new_dist < self.backward_dist[edge.target as usize] {
                        self.mark_touched(edge.target);
                        self.backward_dist[edge.target as usize] = new_dist;
                        self.backward_parent[edge.target as usize] = (node, edge_idx);
                        backward_heap.push(Reverse((new_dist, edge.target)));
                    }
                }
            }
        }

        if best_meeting == u32::MAX {
            return None;
        }

        let path_edges_forward = self.trace_forward_path(ch, source, best_meeting);
        let path_edges_backward = self.trace_backward_path(ch, target, best_meeting);

        let (total_dist, total_dur) =
            Self::compute_totals(ch, &path_edges_forward, &path_edges_backward);

        Some(RouteResult {
            distance_m: total_dist,
            duration_ds: total_dur,
            path_edges_forward,
            path_edges_backward,
            meeting_node: best_meeting,
        })
    }

    fn trace_forward_path(&self, _ch: &ChGraph, source: u32, meeting: u32) -> Vec<(u32, usize)> {
        let mut path = Vec::new();
        let mut current = meeting;
        while current != source {
            let (parent, edge_idx) = self.forward_parent[current as usize];
            if parent == u32::MAX {
                break;
            }
            path.push((parent, edge_idx));
            current = parent;
        }
        path.reverse();
        path
    }

    fn trace_backward_path(&self, _ch: &ChGraph, target: u32, meeting: u32) -> Vec<(u32, usize)> {
        let mut path = Vec::new();
        let mut current = meeting;
        while current != target {
            let (parent, edge_idx) = self.backward_parent[current as usize];
            if parent == u32::MAX {
                break;
            }
            path.push((parent, edge_idx));
            current = parent;
        }
        path
    }

    fn compute_totals(
        ch: &ChGraph,
        forward_path: &[(u32, usize)],
        backward_path: &[(u32, usize)],
    ) -> (u32, u32) {
        let mut total_dist: u32 = 0;
        let mut total_dur: u32 = 0;

        for &(_, edge_idx) in forward_path {
            let edge = &ch.forward_graph.edges[edge_idx];
            total_dist = total_dist.saturating_add(edge.distance_m);
            total_dur = total_dur.saturating_add(edge.time_ds);
        }

        for &(_, edge_idx) in backward_path {
            let edge = &ch.backward_graph.edges[edge_idx];
            total_dist = total_dist.saturating_add(edge.distance_m);
            total_dur = total_dur.saturating_add(edge.time_ds);
        }

        (total_dist, total_dur)
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
    fn query_same_source_target() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let result = query.route(&ch, 0, 0).unwrap();
        assert_eq!(result.distance_m, 0);
        assert_eq!(result.duration_ds, 0);
        assert_eq!(result.meeting_node, 0);
    }

    #[test]
    fn query_triangle_finds_route() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let result = query.route(&ch, 0, 2);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.distance_m, 200);
        assert_eq!(result.duration_ds, 90);
    }

    #[test]
    fn query_linear_graph() {
        let (graph, geometry) = build_linear_graph(5);
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let r01 = query.route(&ch, 0, 1);
        assert!(r01.is_some());

        let r04 = query.route(&ch, 0, 4);
        assert!(r04.is_some());
        let r04 = r04.unwrap();
        assert!(r04.distance_m > 0);
    }

    #[test]
    fn query_disconnected_returns_none() {
        let (graph, geometry) = build_disconnected_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let result = query.route(&ch, 0, 2);
        assert!(result.is_none());
    }

    #[test]
    fn query_reset_works_across_calls() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let r1 = query.route(&ch, 0, 2);
        assert!(r1.is_some());

        let r2 = query.route(&ch, 0, 0);
        assert!(r2.is_some());
        assert_eq!(r2.unwrap().distance_m, 0);

        let r3 = query.route(&ch, 0, 2);
        assert!(r3.is_some());
        assert_eq!(r1.unwrap().distance_m, r3.unwrap().distance_m);
    }

    #[test]
    fn query_out_of_bounds_returns_none() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);
        let mut query = ChQuery::new(ch.forward_graph.num_nodes());

        let result = query.route(&ch, 999, 0);
        assert!(result.is_none());
    }
}
