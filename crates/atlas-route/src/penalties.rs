use std::collections::HashMap;

use atlas_core::{AtlasError, ContributionStore, ContributionType};

use crate::snap::NodeSnapper;

pub struct PenaltyOverlay {
    edge_penalties: HashMap<(u32, u32), f64>,
}

impl PenaltyOverlay {
    pub fn empty() -> Self {
        Self {
            edge_penalties: HashMap::new(),
        }
    }

    pub fn from_contributions(
        store: &ContributionStore,
        snapper: &NodeSnapper,
    ) -> Result<Self, AtlasError> {
        let contributions = store.list()?;
        let mut penalties: HashMap<(u32, u32), f64> = HashMap::new();

        for contrib in &contributions {
            let src = snapper.snap(contrib.route_origin.lat, contrib.route_origin.lon);
            let dst = snapper.snap(contrib.route_destination.lat, contrib.route_destination.lon);

            let (src, dst) = match (src, dst) {
                (Some(s), Some(d)) => (s, d),
                _ => continue,
            };

            match contrib.issue_type {
                ContributionType::RoadClosed => {
                    penalties.insert((src, dst), f64::INFINITY);
                    penalties.insert((dst, src), f64::INFINITY);
                }
                ContributionType::SpeedWrong => {
                    penalties.insert((src, dst), 1.5);
                    penalties.insert((dst, src), 1.5);
                }
                ContributionType::WrongTurn | ContributionType::RoundaboutError => {
                    penalties.insert((src, dst), 1.2);
                }
                ContributionType::BetterRoute
                | ContributionType::MissingRoad
                | ContributionType::Other => {}
            }
        }

        Ok(Self {
            edge_penalties: penalties,
        })
    }

    pub fn get_penalty(&self, source: u32, target: u32) -> f64 {
        *self.edge_penalties.get(&(source, target)).unwrap_or(&1.0)
    }

    pub fn is_blocked(&self, source: u32, target: u32) -> bool {
        self.get_penalty(source, target).is_infinite()
    }

    pub fn num_penalties(&self) -> usize {
        self.edge_penalties.len()
    }

    pub fn from_map(penalties: HashMap<(u32, u32), f64>) -> Self {
        Self {
            edge_penalties: penalties,
        }
    }

    pub fn merge_with(&self, other: &PenaltyOverlay) -> PenaltyOverlay {
        let mut merged = self.edge_penalties.clone();
        for (&key, &penalty) in &other.edge_penalties {
            let existing = merged.entry(key).or_insert(1.0);
            *existing = (*existing).max(penalty);
        }
        PenaltyOverlay {
            edge_penalties: merged,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dijkstra::dijkstra_astar;
    use crate::graph::edge::{make_flags, Access, Edge, RoadClass, Surface};
    use crate::graph::road_network::RoadGraph;
    use crate::profiles::CarProfile;
    use crate::snap::NodeSnapper;
    use atlas_core::{ContributionStore, ContributionType, LatLon, RouteContribution};

    #[test]
    fn empty_overlay_returns_1() {
        let overlay = PenaltyOverlay::empty();
        assert_eq!(overlay.get_penalty(0, 1), 1.0);
        assert_eq!(overlay.get_penalty(999, 888), 1.0);
        assert!(!overlay.is_blocked(0, 1));
        assert_eq!(overlay.num_penalties(), 0);
    }

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

    fn make_contribution(
        id: &str,
        origin: (f64, f64),
        dest: (f64, f64),
        issue_type: ContributionType,
    ) -> RouteContribution {
        RouteContribution {
            id: id.to_string(),
            route_origin: LatLon {
                lat: origin.0,
                lon: origin.1,
            },
            route_destination: LatLon {
                lat: dest.0,
                lon: dest.1,
            },
            profile: "car".to_string(),
            issue_type,
            description: None,
            suggested_waypoints: None,
            created_at: "2026-03-17T10:00:00Z".to_string(),
        }
    }

    fn setup_store_with(
        tag: &str,
        contributions: &[RouteContribution],
    ) -> (ContributionStore, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("atlas-penalty-test-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = ContributionStore::new(&dir);
        for c in contributions {
            store.save(c).unwrap();
        }
        (store, dir)
    }

    #[test]
    fn road_closed_blocks_edge() {
        let graph = RoadGraph {
            first_edge: vec![0, 0, 0, 0],
            edges: Vec::new(),
            node_lat: vec![5.0, 5.1, 5.2],
            node_lon: vec![-0.1, -0.2, -0.3],
        };
        let snapper = NodeSnapper::build(&graph);

        let contrib = make_contribution(
            "closed-1",
            (5.001, -0.101),
            (5.101, -0.201),
            ContributionType::RoadClosed,
        );
        let (store, dir) = setup_store_with("closed", &[contrib]);

        let overlay = PenaltyOverlay::from_contributions(&store, &snapper).unwrap();
        assert!(overlay.is_blocked(0, 1));
        assert!(overlay.is_blocked(1, 0));
        assert!(!overlay.is_blocked(0, 2));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn speed_wrong_penalizes_edge() {
        let graph = RoadGraph {
            first_edge: vec![0, 0, 0, 0],
            edges: Vec::new(),
            node_lat: vec![5.0, 5.1, 5.2],
            node_lon: vec![-0.1, -0.2, -0.3],
        };
        let snapper = NodeSnapper::build(&graph);

        let contrib = make_contribution(
            "speed-1",
            (5.001, -0.101),
            (5.101, -0.201),
            ContributionType::SpeedWrong,
        );
        let (store, dir) = setup_store_with("speed", &[contrib]);

        let overlay = PenaltyOverlay::from_contributions(&store, &snapper).unwrap();
        assert_eq!(overlay.get_penalty(0, 1), 1.5);
        assert_eq!(overlay.get_penalty(1, 0), 1.5);
        assert_eq!(overlay.get_penalty(0, 2), 1.0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dijkstra_avoids_blocked_edge() {
        let flags = test_flags();

        let edges = vec![
            Edge {
                target: 1,
                geo_index: 0,
                shortcut_mid: 0,
                distance_m: 500,
                time_ds: 225,
                flags,
                _padding: 0,
            },
            Edge {
                target: 2,
                geo_index: 1,
                shortcut_mid: 0,
                distance_m: 3000,
                time_ds: 1350,
                flags,
                _padding: 0,
            },
            Edge {
                target: 2,
                geo_index: 2,
                shortcut_mid: 0,
                distance_m: 500,
                time_ds: 225,
                flags,
                _padding: 0,
            },
        ];

        let graph = RoadGraph {
            first_edge: vec![0, 2, 3, 3],
            edges,
            node_lat: vec![5.600, 5.601, 5.602],
            node_lon: vec![-0.200, -0.201, -0.202],
        };

        let without_penalty = dijkstra_astar(&graph, &CarProfile, 0, 2, None).unwrap();
        assert_eq!(without_penalty.path_edges.len(), 2);

        let mut penalties_map = HashMap::new();
        penalties_map.insert((0u32, 1u32), f64::INFINITY);
        let overlay = PenaltyOverlay {
            edge_penalties: penalties_map,
        };

        let with_penalty = dijkstra_astar(&graph, &CarProfile, 0, 2, Some(&overlay)).unwrap();
        assert_eq!(with_penalty.path_edges.len(), 1);
        assert!(with_penalty.duration_ds > without_penalty.duration_ds);
    }
}
