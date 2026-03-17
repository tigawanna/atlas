use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::graph::edge::{encode_shortcut_mid, Edge, NO_SHORTCUT};
use crate::graph::road_network::{RoadGeometry, RoadGraph};
use crate::profiles::RoutingProfile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChGraph {
    pub forward_graph: RoadGraph,
    pub backward_graph: RoadGraph,
    pub geometry: RoadGeometry,
    pub ch_level: Vec<u16>,
    pub num_original_nodes: u32,
    pub profile_name: String,
}

#[derive(Debug, Clone)]
struct TempEdge {
    target: u32,
    weight: u32,
    distance_m: u32,
    original_edge_idx: Option<usize>,
    shortcut_mid: u32,
}

const WITNESS_SEARCH_LIMIT: usize = 20;

pub fn build_ch(
    graph: &RoadGraph,
    geometry: &RoadGeometry,
    profile: &dyn RoutingProfile,
) -> ChGraph {
    let num_nodes = graph.num_nodes();

    let (mut forward_adj, mut backward_adj) = build_mutable_adj(graph, profile);

    let (ch_level, contraction_order) =
        contract_nodes(&mut forward_adj, &mut backward_adj, num_nodes);

    let _ = contraction_order;

    let (fwd_graph, bwd_graph) =
        build_ch_graphs(graph, &forward_adj, &backward_adj, &ch_level, num_nodes);

    ChGraph {
        forward_graph: fwd_graph,
        backward_graph: bwd_graph,
        geometry: geometry.clone(),
        ch_level,
        num_original_nodes: num_nodes as u32,
        profile_name: profile.name().to_string(),
    }
}

fn build_mutable_adj(
    graph: &RoadGraph,
    profile: &dyn RoutingProfile,
) -> (Vec<Vec<TempEdge>>, Vec<Vec<TempEdge>>) {
    let num_nodes = graph.num_nodes();
    let mut forward_adj: Vec<Vec<TempEdge>> = vec![Vec::new(); num_nodes];
    let mut backward_adj: Vec<Vec<TempEdge>> = vec![Vec::new(); num_nodes];

    #[allow(clippy::needless_range_loop)]
    for source in 0..num_nodes {
        for edge in graph.edges_of(source as u32) {
            let weight = match profile.edge_weight(edge) {
                Some(w) => w,
                None => continue,
            };
            let target = edge.target;
            let edge_idx = (edge as *const Edge as usize - graph.edges.as_ptr() as usize)
                / std::mem::size_of::<Edge>();

            forward_adj[source].push(TempEdge {
                target,
                weight,
                distance_m: edge.distance_m,
                original_edge_idx: Some(edge_idx),
                shortcut_mid: NO_SHORTCUT,
            });

            backward_adj[target as usize].push(TempEdge {
                target: source as u32,
                weight,
                distance_m: edge.distance_m,
                original_edge_idx: Some(edge_idx),
                shortcut_mid: NO_SHORTCUT,
            });
        }
    }

    (forward_adj, backward_adj)
}

fn compute_priority(
    node: u32,
    forward_adj: &[Vec<TempEdge>],
    backward_adj: &[Vec<TempEdge>],
    contracted: &[bool],
    ch_level: &[u16],
) -> i64 {
    let in_edges: Vec<&TempEdge> = backward_adj[node as usize]
        .iter()
        .filter(|e| !contracted[e.target as usize])
        .collect();
    let out_edges: Vec<&TempEdge> = forward_adj[node as usize]
        .iter()
        .filter(|e| !contracted[e.target as usize])
        .collect();

    let edges_removed = (in_edges.len() + out_edges.len()) as i64;

    let mut shortcuts_needed = 0i64;
    for in_e in &in_edges {
        for out_e in &out_edges {
            if in_e.target == out_e.target {
                continue;
            }
            let weight_uvw = in_e.weight.saturating_add(out_e.weight);
            if !witness_search(
                forward_adj,
                contracted,
                in_e.target,
                out_e.target,
                node,
                weight_uvw,
            ) {
                shortcuts_needed += 1;
            }
        }
    }

    let edge_difference = shortcuts_needed - edges_removed;

    let mut max_neighbor_level = 0u16;
    let mut num_contracted_neighbors = 0i64;
    for e in backward_adj[node as usize]
        .iter()
        .chain(forward_adj[node as usize].iter())
    {
        if contracted[e.target as usize] {
            num_contracted_neighbors += 1;
        } else {
            max_neighbor_level = max_neighbor_level.max(ch_level[e.target as usize]);
        }
    }

    edge_difference + 2 * max_neighbor_level as i64 + num_contracted_neighbors
}

fn witness_search(
    forward_adj: &[Vec<TempEdge>],
    contracted: &[bool],
    source: u32,
    target: u32,
    exclude: u32,
    max_weight: u32,
) -> bool {
    if source == target {
        return true;
    }

    let mut dist: Vec<u32> = Vec::new();
    let mut visited_nodes: Vec<u32> = Vec::new();
    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
    let mut settled = 0usize;

    let capacity = forward_adj.len();
    dist.resize(capacity, u32::MAX);

    dist[source as usize] = 0;
    visited_nodes.push(source);
    heap.push(Reverse((0, source)));

    while let Some(Reverse((cost, node))) = heap.pop() {
        if cost > dist[node as usize] {
            continue;
        }
        if node == target {
            for &v in &visited_nodes {
                dist[v as usize] = u32::MAX;
            }
            return cost <= max_weight;
        }

        settled += 1;
        if settled > WITNESS_SEARCH_LIMIT || cost > max_weight {
            for &v in &visited_nodes {
                dist[v as usize] = u32::MAX;
            }
            return false;
        }

        for edge in &forward_adj[node as usize] {
            if edge.target == exclude || contracted[edge.target as usize] {
                continue;
            }
            let new_cost = cost.saturating_add(edge.weight);
            if new_cost < dist[edge.target as usize] {
                dist[edge.target as usize] = new_cost;
                visited_nodes.push(edge.target);
                if edge.target == target && new_cost <= max_weight {
                    for &v in &visited_nodes {
                        dist[v as usize] = u32::MAX;
                    }
                    return true;
                }
                heap.push(Reverse((new_cost, edge.target)));
            }
        }
    }

    for &v in &visited_nodes {
        dist[v as usize] = u32::MAX;
    }
    false
}

fn witness_search_local(
    forward_adj: &[Vec<TempEdge>],
    contracted: &[bool],
    source: u32,
    target: u32,
    exclude: u32,
    max_weight: u32,
) -> bool {
    if source == target {
        return true;
    }

    let mut dist: HashMap<u32, u32> = HashMap::with_capacity(WITNESS_SEARCH_LIMIT + 1);
    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
    let mut settled = 0usize;

    dist.insert(source, 0);
    heap.push(Reverse((0, source)));

    while let Some(Reverse((cost, node))) = heap.pop() {
        if cost > *dist.get(&node).unwrap_or(&u32::MAX) {
            continue;
        }
        if node == target {
            return cost <= max_weight;
        }

        settled += 1;
        if settled > WITNESS_SEARCH_LIMIT || cost > max_weight {
            return false;
        }

        for edge in &forward_adj[node as usize] {
            if edge.target == exclude || contracted[edge.target as usize] {
                continue;
            }
            let new_cost = cost.saturating_add(edge.weight);
            let current = dist.get(&edge.target).copied().unwrap_or(u32::MAX);
            if new_cost < current {
                dist.insert(edge.target, new_cost);
                if edge.target == target && new_cost <= max_weight {
                    return true;
                }
                heap.push(Reverse((new_cost, edge.target)));
            }
        }
    }

    false
}

fn contract_nodes(
    forward_adj: &mut [Vec<TempEdge>],
    backward_adj: &mut [Vec<TempEdge>],
    num_nodes: usize,
) -> (Vec<u16>, Vec<u32>) {
    let mut contracted = vec![false; num_nodes];
    let mut ch_level = vec![0u16; num_nodes];
    let mut contraction_order = Vec::with_capacity(num_nodes);

    let mut heap: BinaryHeap<Reverse<(i64, u32)>> = BinaryHeap::new();
    for node in 0..num_nodes {
        let priority = compute_priority(
            node as u32,
            forward_adj,
            backward_adj,
            &contracted,
            &ch_level,
        );
        heap.push(Reverse((priority, node as u32)));
    }

    let mut level: u16 = 0;
    let mut shortcuts_added: u64 = 0;

    while let Some(Reverse((old_priority, node))) = heap.pop() {
        if contracted[node as usize] {
            continue;
        }

        let new_priority =
            compute_priority(node, forward_adj, backward_adj, &contracted, &ch_level);

        if let Some(&Reverse((next_priority, _))) = heap.peek() {
            if new_priority > old_priority && new_priority > next_priority {
                heap.push(Reverse((new_priority, node)));
                continue;
            }
        }

        let in_edges: Vec<(u32, u32, u32)> = backward_adj[node as usize]
            .iter()
            .filter(|e| !contracted[e.target as usize])
            .map(|e| (e.target, e.weight, e.distance_m))
            .collect();

        let out_edges: Vec<(u32, u32, u32)> = forward_adj[node as usize]
            .iter()
            .filter(|e| !contracted[e.target as usize])
            .map(|e| (e.target, e.weight, e.distance_m))
            .collect();

        let pairs: Vec<(u32, u32, u32, u32, u32, u32)> = in_edges
            .iter()
            .flat_map(|&(u, w_uv, d_uv)| {
                out_edges
                    .iter()
                    .map(move |&(w, w_vw, d_vw)| (u, w_uv, d_uv, w, w_vw, d_vw))
            })
            .filter(|&(u, _, _, w, _, _)| u != w)
            .collect();

        let fwd_ref: &[Vec<TempEdge>] = forward_adj;
        let contracted_ref: &[bool] = &contracted;

        let shortcuts_needed: Vec<(u32, u32, u32, u32)> = pairs
            .par_iter()
            .filter_map(|&(u, w_uv, d_uv, w, w_vw, d_vw)| {
                let weight_uvw = w_uv.saturating_add(w_vw);
                if !witness_search_local(fwd_ref, contracted_ref, u, w, node, weight_uvw) {
                    Some((u, w, weight_uvw, d_uv.saturating_add(d_vw)))
                } else {
                    None
                }
            })
            .collect();

        for (u, w, weight_uvw, distance_uvw) in shortcuts_needed {
            forward_adj[u as usize].push(TempEdge {
                target: w,
                weight: weight_uvw,
                distance_m: distance_uvw,
                original_edge_idx: None,
                shortcut_mid: node,
            });
            backward_adj[w as usize].push(TempEdge {
                target: u,
                weight: weight_uvw,
                distance_m: distance_uvw,
                original_edge_idx: None,
                shortcut_mid: node,
            });
            shortcuts_added += 1;
        }

        contracted[node as usize] = true;
        ch_level[node as usize] = level;
        contraction_order.push(node);

        if contraction_order.len() % 10_000 == 0 {
            let pct = (contraction_order.len() as f64 / num_nodes as f64) * 100.0;
            tracing::info!(
                contracted = contraction_order.len(),
                total = num_nodes,
                percent = format!("{pct:.1}"),
                shortcuts = shortcuts_added,
                "CH contraction progress"
            );
        }

        level = level.saturating_add(1);
    }

    (ch_level, contraction_order)
}

fn build_ch_graphs(
    graph: &RoadGraph,
    forward_adj: &[Vec<TempEdge>],
    backward_adj: &[Vec<TempEdge>],
    ch_level: &[u16],
    num_nodes: usize,
) -> (RoadGraph, RoadGraph) {
    let mut fwd_raw: Vec<(u32, Edge)> = Vec::new();
    let mut bwd_raw: Vec<(u32, Edge)> = Vec::new();

    for u in 0..num_nodes {
        for edge in &forward_adj[u] {
            let w = edge.target as usize;
            if ch_level[w] >= ch_level[u] {
                let edge = build_ch_edge(graph, edge);
                fwd_raw.push((u as u32, edge));
            }
        }

        for edge in &backward_adj[u] {
            let w = edge.target as usize;
            if ch_level[w] >= ch_level[u] {
                let edge = build_ch_edge(graph, edge);
                bwd_raw.push((u as u32, edge));
            }
        }
    }

    fwd_raw.sort_by_key(|&(src, _)| src);
    bwd_raw.sort_by_key(|&(src, _)| src);

    let fwd_graph = edges_to_road_graph(&fwd_raw, num_nodes, &graph.node_lat, &graph.node_lon);
    let bwd_graph = edges_to_road_graph(&bwd_raw, num_nodes, &graph.node_lat, &graph.node_lon);

    (fwd_graph, bwd_graph)
}

fn build_ch_edge(graph: &RoadGraph, edge: &TempEdge) -> Edge {
    if let Some(edge_idx) = edge.original_edge_idx {
        let original = graph.edges[edge_idx];
        return Edge {
            target: edge.target,
            geo_index: original.geo_index,
            shortcut_mid: NO_SHORTCUT,
            distance_m: original.distance_m,
            time_ds: edge.weight,
            flags: original.flags,
            _padding: original._padding,
        };
    }

    Edge {
        target: edge.target,
        geo_index: 0,
        shortcut_mid: encode_shortcut_mid(edge.shortcut_mid),
        distance_m: edge.distance_m,
        time_ds: edge.weight,
        flags: 0,
        _padding: 0,
    }
}

fn edges_to_road_graph(
    sorted_edges: &[(u32, Edge)],
    num_nodes: usize,
    node_lat: &[f32],
    node_lon: &[f32],
) -> RoadGraph {
    let mut first_edge = vec![0u32; num_nodes + 1];
    let mut edges = Vec::with_capacity(sorted_edges.len());

    let mut current_node = 0u32;
    for (idx, &(src, ref edge)) in sorted_edges.iter().enumerate() {
        while current_node <= src {
            first_edge[current_node as usize] = idx as u32;
            current_node += 1;
        }
        edges.push(*edge);
    }
    while (current_node as usize) <= num_nodes {
        first_edge[current_node as usize] = edges.len() as u32;
        current_node += 1;
    }

    RoadGraph {
        first_edge,
        edges,
        node_lat: node_lat.to_vec(),
        node_lon: node_lon.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::edge::{make_flags, Access, RoadClass, Surface};
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
    fn ch_contracts_triangle() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);

        assert_eq!(ch.ch_level.len(), 3);
        assert_eq!(ch.num_original_nodes, 3);
        assert_eq!(ch.profile_name, "car");

        let mut levels: Vec<u16> = ch.ch_level.clone();
        levels.sort();
        assert_eq!(levels, vec![0, 1, 2]);
    }

    #[test]
    fn ch_forward_backward_nonempty() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);

        assert!(ch.forward_graph.num_edges() > 0);
        assert!(ch.backward_graph.num_edges() > 0);
        assert_eq!(ch.forward_graph.num_nodes(), 3);
        assert_eq!(ch.backward_graph.num_nodes(), 3);
    }

    #[test]
    fn ch_has_upward_edges() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);

        for node in 0..ch.forward_graph.num_nodes() {
            for edge in ch.forward_graph.edges_of(node as u32) {
                assert!(
                    ch.ch_level[edge.target as usize] >= ch.ch_level[node],
                    "forward edge {}->{} violates upward property",
                    node,
                    edge.target
                );
            }
        }
    }

    #[test]
    fn ch_has_backward_upward_edges() {
        let (graph, geometry) = build_triangle_graph();
        let ch = build_ch(&graph, &geometry, &CarProfile);

        for node in 0..ch.backward_graph.num_nodes() {
            for edge in ch.backward_graph.edges_of(node as u32) {
                assert!(
                    ch.ch_level[edge.target as usize] >= ch.ch_level[node],
                    "backward edge {}->{} violates upward property",
                    node,
                    edge.target
                );
            }
        }
    }

    #[test]
    fn witness_search_finds_direct_path() {
        let mut adj: Vec<Vec<TempEdge>> = vec![Vec::new(); 3];
        let contracted = vec![false; 3];

        adj[0].push(TempEdge {
            target: 2,
            weight: 5,
            distance_m: 5,
            original_edge_idx: None,
            shortcut_mid: NO_SHORTCUT,
        });
        adj[0].push(TempEdge {
            target: 1,
            weight: 3,
            distance_m: 3,
            original_edge_idx: None,
            shortcut_mid: NO_SHORTCUT,
        });
        adj[1].push(TempEdge {
            target: 2,
            weight: 3,
            distance_m: 3,
            original_edge_idx: None,
            shortcut_mid: NO_SHORTCUT,
        });

        assert!(witness_search(&adj, &contracted, 0, 2, 1, 5));
        assert!(!witness_search(&adj, &contracted, 0, 2, 1, 4));
    }

    #[test]
    fn witness_search_excludes_node() {
        let mut adj: Vec<Vec<TempEdge>> = vec![Vec::new(); 3];
        let contracted = vec![false; 3];

        adj[0].push(TempEdge {
            target: 1,
            weight: 3,
            distance_m: 3,
            original_edge_idx: None,
            shortcut_mid: NO_SHORTCUT,
        });
        adj[1].push(TempEdge {
            target: 2,
            weight: 3,
            distance_m: 3,
            original_edge_idx: None,
            shortcut_mid: NO_SHORTCUT,
        });

        assert!(!witness_search(&adj, &contracted, 0, 2, 1, 10));
    }

    fn build_linear_graph(num_nodes: usize) -> (RoadGraph, RoadGeometry) {
        let flags = make_flags(
            RoadClass::Primary,
            Surface::Paved,
            false,
            false,
            Access::Yes,
            Access::Yes,
            false,
        );

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

    #[test]
    fn ch_linear_graph() {
        let (graph, geometry) = build_linear_graph(5);
        let ch = build_ch(&graph, &geometry, &CarProfile);

        assert_eq!(ch.ch_level.len(), 5);
        assert_eq!(ch.num_original_nodes, 5);

        let mut levels = ch.ch_level.clone();
        levels.sort();
        levels.dedup();
        assert_eq!(levels.len(), 5);
    }
}
