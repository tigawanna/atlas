use serde::{Deserialize, Serialize};

use super::edge::Edge;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoadGraph {
    pub first_edge: Vec<u32>,
    pub edges: Vec<Edge>,
    pub node_lat: Vec<f32>,
    pub node_lon: Vec<f32>,
}

impl RoadGraph {
    pub fn num_nodes(&self) -> usize {
        self.node_lat.len()
    }

    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    pub fn edges_of(&self, node: u32) -> &[Edge] {
        let node = node as usize;
        let start = self.first_edge[node] as usize;
        let end = self.first_edge[node + 1] as usize;
        &self.edges[start..end]
    }

    pub fn node_coords(&self, node: u32) -> (f32, f32) {
        let node = node as usize;
        (self.node_lat[node], self.node_lon[node])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoadGeometry {
    pub first_point: Vec<u32>,
    pub coords_lat: Vec<f32>,
    pub coords_lon: Vec<f32>,
    pub road_names: Vec<Option<String>>,
}

impl RoadGeometry {
    pub fn num_segments(&self) -> usize {
        self.road_names.len()
    }

    pub fn get_points(&self, geo_index: u32) -> (&[f32], &[f32]) {
        let idx = geo_index as usize;
        let start = self.first_point[idx] as usize;
        let end = self.first_point[idx + 1] as usize;
        (&self.coords_lat[start..end], &self.coords_lon[start..end])
    }

    pub fn get_road_name(&self, geo_index: u32) -> Option<&str> {
        self.road_names[geo_index as usize].as_deref()
    }
}
