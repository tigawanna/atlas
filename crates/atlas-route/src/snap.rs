use atlas_core::geo_utils::haversine_distance;
use rstar::{RTree, RTreeObject, AABB};

use crate::graph::RoadGraph;

const MAX_SNAP_RADIUS_M: f64 = 1000.0;

#[derive(Debug, Clone)]
struct GraphNode {
    node_id: u32,
    lon: f32,
    lat: f32,
}

impl RTreeObject for GraphNode {
    type Envelope = AABB<[f32; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point([self.lon, self.lat])
    }
}

pub struct NodeSnapper {
    rtree: RTree<GraphNode>,
}

impl NodeSnapper {
    pub fn build(graph: &RoadGraph) -> Self {
        let nodes: Vec<GraphNode> = (0..graph.num_nodes() as u32)
            .map(|i| {
                let (lat, lon) = graph.node_coords(i);
                GraphNode {
                    node_id: i,
                    lon,
                    lat,
                }
            })
            .collect();
        Self {
            rtree: RTree::bulk_load(nodes),
        }
    }

    pub fn snap(&self, lat: f64, lon: f64) -> Option<u32> {
        let deg_offset = MAX_SNAP_RADIUS_M / 111_320.0;
        let lon_f32 = lon as f32;
        let lat_f32 = lat as f32;
        let deg_f32 = deg_offset as f32;

        let envelope = AABB::from_corners(
            [lon_f32 - deg_f32, lat_f32 - deg_f32],
            [lon_f32 + deg_f32, lat_f32 + deg_f32],
        );

        let mut best_node: Option<u32> = None;
        let mut best_dist = f64::MAX;

        for node in self.rtree.locate_in_envelope_intersecting(&envelope) {
            let dist = haversine_distance(lat, lon, node.lat as f64, node.lon as f64);
            if dist < best_dist && dist <= MAX_SNAP_RADIUS_M {
                best_dist = dist;
                best_node = Some(node.node_id);
            }
        }

        best_node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::road_network::RoadGraph;

    fn make_test_graph() -> RoadGraph {
        RoadGraph {
            first_edge: vec![0, 0, 0, 0],
            edges: Vec::new(),
            node_lat: vec![5.0, 5.1, 5.2],
            node_lon: vec![-0.1, -0.2, -0.3],
        }
    }

    #[test]
    fn snap_finds_nearest_node() {
        let graph = make_test_graph();
        let snapper = NodeSnapper::build(&graph);

        let result = snapper.snap(5.001, -0.101);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn snap_returns_none_when_too_far() {
        let graph = make_test_graph();
        let snapper = NodeSnapper::build(&graph);

        let result = snapper.snap(10.0, 10.0);
        assert!(result.is_none());
    }

    #[test]
    fn snap_exact_coordinates() {
        let graph = make_test_graph();
        let snapper = NodeSnapper::build(&graph);

        let result = snapper.snap(5.1, -0.2);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn snap_picks_closest_of_multiple() {
        let graph = RoadGraph {
            first_edge: vec![0, 0, 0, 0],
            edges: Vec::new(),
            node_lat: vec![5.000, 5.002, 5.004],
            node_lon: vec![-0.100, -0.102, -0.104],
        };
        let snapper = NodeSnapper::build(&graph);

        let result = snapper.snap(5.0019, -0.1019);
        assert_eq!(result, Some(1));
    }
}
