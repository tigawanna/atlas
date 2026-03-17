use super::africa::{apply_africa_modifier, speed_to_deciseconds};
use super::RoutingProfile;
use crate::graph::{Access, Edge, RoadClass};

pub struct FootProfile;

const FOOT_SPEEDS: [f64; 8] = [0.0, 0.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0];

impl RoutingProfile for FootProfile {
    fn name(&self) -> &str {
        "foot"
    }

    fn is_accessible(&self, edge: &Edge) -> bool {
        if edge.is_seasonal_closure() {
            return false;
        }
        !matches!(edge.road_class(), RoadClass::Motorway | RoadClass::Trunk)
            && !matches!(edge.access_foot(), Access::No)
    }

    fn edge_weight(&self, edge: &Edge) -> Option<u32> {
        if !self.is_accessible(edge) {
            return None;
        }
        let base_speed = FOOT_SPEEDS[edge.road_class() as usize];
        if base_speed <= 0.0 {
            return None;
        }
        let speed = apply_africa_modifier(base_speed, edge)?;
        Some(speed_to_deciseconds(edge.distance_m, speed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{make_flags, Access, RoadClass, Surface};

    fn edge_with(road_class: RoadClass, surface: Surface, oneway: bool, seasonal: bool) -> Edge {
        Edge {
            target: 0,
            geo_index: 0,
            shortcut_mid: 0,
            distance_m: 1000,
            time_ds: 0,
            flags: make_flags(
                road_class,
                surface,
                oneway,
                seasonal,
                Access::Yes,
                Access::Yes,
                false,
            ),
            _padding: 0,
        }
    }

    #[test]
    fn foot_ignores_oneway() {
        let oneway_edge = edge_with(RoadClass::Primary, Surface::Paved, true, false);
        assert!(FootProfile.is_accessible(&oneway_edge));
        assert!(FootProfile.edge_weight(&oneway_edge).is_some());
    }

    #[test]
    fn foot_respects_access_restrictions() {
        let edge = Edge {
            target: 0,
            geo_index: 0,
            shortcut_mid: 0,
            distance_m: 1000,
            time_ds: 0,
            flags: make_flags(
                RoadClass::Residential,
                Surface::Paved,
                false,
                false,
                Access::No,
                Access::Yes,
                false,
            ),
            _padding: 0,
        };
        assert!(!FootProfile.is_accessible(&edge));
        assert!(FootProfile.edge_weight(&edge).is_none());
    }

    #[test]
    fn foot_rejects_motorway() {
        let edge = edge_with(RoadClass::Motorway, Surface::Paved, false, false);
        assert!(!FootProfile.is_accessible(&edge));
        assert!(FootProfile.edge_weight(&edge).is_none());
    }

    #[test]
    fn foot_rejects_trunk() {
        let edge = edge_with(RoadClass::Trunk, Surface::Paved, false, false);
        assert!(!FootProfile.is_accessible(&edge));
    }

    #[test]
    fn unpaved_penalty_applied() {
        let paved = edge_with(RoadClass::Primary, Surface::Paved, false, false);
        let unpaved = edge_with(RoadClass::Primary, Surface::Unpaved, false, false);
        let paved_time = FootProfile.edge_weight(&paved).unwrap();
        let unpaved_time = FootProfile.edge_weight(&unpaved).unwrap();
        assert!(unpaved_time > paved_time);
    }

    #[test]
    fn seasonal_closure_inaccessible() {
        let edge = edge_with(RoadClass::Primary, Surface::Paved, false, true);
        assert!(!FootProfile.is_accessible(&edge));
        assert!(FootProfile.edge_weight(&edge).is_none());
    }
}
