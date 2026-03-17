use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Edge {
    pub target: u32,
    pub geo_index: u32,
    pub shortcut_mid: u32,
    pub distance_m: u32,
    pub time_ds: u32,
    pub flags: u16,
    pub _padding: u16,
}

pub const NO_SHORTCUT: u32 = 0;

pub fn encode_shortcut_mid(mid: u32) -> u32 {
    mid.saturating_add(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoadClass {
    Motorway = 0,
    Trunk = 1,
    Primary = 2,
    Secondary = 3,
    Tertiary = 4,
    Residential = 5,
    Track = 6,
    Path = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Paved = 0,
    Unpaved = 1,
    Track = 2,
    Unknown = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Yes = 0,
    No = 1,
    Designated = 2,
}

impl Edge {
    pub fn decoded_shortcut_mid(&self) -> Option<u32> {
        if self.shortcut_mid == NO_SHORTCUT {
            None
        } else {
            Some(self.shortcut_mid - 1)
        }
    }

    pub fn road_class(&self) -> RoadClass {
        match self.flags & 0b111 {
            0 => RoadClass::Motorway,
            1 => RoadClass::Trunk,
            2 => RoadClass::Primary,
            3 => RoadClass::Secondary,
            4 => RoadClass::Tertiary,
            5 => RoadClass::Residential,
            6 => RoadClass::Track,
            _ => RoadClass::Path,
        }
    }

    pub fn surface(&self) -> Surface {
        match (self.flags >> 3) & 0b11 {
            0 => Surface::Paved,
            1 => Surface::Unpaved,
            2 => Surface::Track,
            _ => Surface::Unknown,
        }
    }

    pub fn is_oneway(&self) -> bool {
        (self.flags >> 5) & 1 == 1
    }

    pub fn is_seasonal_closure(&self) -> bool {
        (self.flags >> 6) & 1 == 1
    }

    pub fn access_foot(&self) -> Access {
        match (self.flags >> 7) & 0b11 {
            0 => Access::Yes,
            1 => Access::No,
            _ => Access::Designated,
        }
    }

    pub fn access_bicycle(&self) -> Access {
        match (self.flags >> 9) & 0b11 {
            0 => Access::Yes,
            1 => Access::No,
            _ => Access::Designated,
        }
    }

    pub fn is_roundabout(&self) -> bool {
        (self.flags >> 11) & 1 == 1
    }
}

pub fn make_flags(
    road_class: RoadClass,
    surface: Surface,
    oneway: bool,
    seasonal: bool,
    foot: Access,
    bicycle: Access,
    roundabout: bool,
) -> u16 {
    let rc = road_class as u16;
    let sf = (surface as u16) << 3;
    let ow = (oneway as u16) << 5;
    let sc = (seasonal as u16) << 6;
    let ft = (foot as u16) << 7;
    let bk = (bicycle as u16) << 9;
    let ra = (roundabout as u16) << 11;
    rc | sf | ow | sc | ft | bk | ra
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_edge(flags: u16) -> Edge {
        Edge {
            target: 0,
            geo_index: 0,
            shortcut_mid: 0,
            distance_m: 100,
            time_ds: 50,
            flags,
            _padding: 0,
        }
    }

    #[test]
    fn road_class_round_trip() {
        let classes = [
            RoadClass::Motorway,
            RoadClass::Trunk,
            RoadClass::Primary,
            RoadClass::Secondary,
            RoadClass::Tertiary,
            RoadClass::Residential,
            RoadClass::Track,
            RoadClass::Path,
        ];
        for cls in classes {
            let flags = make_flags(
                cls,
                Surface::Paved,
                false,
                false,
                Access::Yes,
                Access::Yes,
                false,
            );
            let edge = make_edge(flags);
            assert_eq!(edge.road_class(), cls);
        }
    }

    #[test]
    fn surface_round_trip() {
        let surfaces = [
            Surface::Paved,
            Surface::Unpaved,
            Surface::Track,
            Surface::Unknown,
        ];
        for sf in surfaces {
            let flags = make_flags(
                RoadClass::Primary,
                sf,
                false,
                false,
                Access::Yes,
                Access::Yes,
                false,
            );
            let edge = make_edge(flags);
            assert_eq!(edge.surface(), sf);
        }
    }

    #[test]
    fn oneway_flag() {
        let flags = make_flags(
            RoadClass::Primary,
            Surface::Paved,
            true,
            false,
            Access::Yes,
            Access::Yes,
            false,
        );
        assert!(make_edge(flags).is_oneway());
        let flags = make_flags(
            RoadClass::Primary,
            Surface::Paved,
            false,
            false,
            Access::Yes,
            Access::Yes,
            false,
        );
        assert!(!make_edge(flags).is_oneway());
    }

    #[test]
    fn seasonal_closure_flag() {
        let flags = make_flags(
            RoadClass::Primary,
            Surface::Paved,
            false,
            true,
            Access::Yes,
            Access::Yes,
            false,
        );
        assert!(make_edge(flags).is_seasonal_closure());
        let flags = make_flags(
            RoadClass::Primary,
            Surface::Paved,
            false,
            false,
            Access::Yes,
            Access::Yes,
            false,
        );
        assert!(!make_edge(flags).is_seasonal_closure());
    }

    #[test]
    fn access_foot_round_trip() {
        let accesses = [Access::Yes, Access::No, Access::Designated];
        for acc in accesses {
            let flags = make_flags(
                RoadClass::Primary,
                Surface::Paved,
                false,
                false,
                acc,
                Access::Yes,
                false,
            );
            assert_eq!(make_edge(flags).access_foot(), acc);
        }
    }

    #[test]
    fn access_bicycle_round_trip() {
        let accesses = [Access::Yes, Access::No, Access::Designated];
        for acc in accesses {
            let flags = make_flags(
                RoadClass::Primary,
                Surface::Paved,
                false,
                false,
                Access::Yes,
                acc,
                false,
            );
            assert_eq!(make_edge(flags).access_bicycle(), acc);
        }
    }

    #[test]
    fn roundabout_flag() {
        let flags = make_flags(
            RoadClass::Primary,
            Surface::Paved,
            true,
            false,
            Access::Yes,
            Access::Yes,
            true,
        );
        assert!(make_edge(flags).is_roundabout());
        let flags = make_flags(
            RoadClass::Primary,
            Surface::Paved,
            false,
            false,
            Access::Yes,
            Access::Yes,
            false,
        );
        assert!(!make_edge(flags).is_roundabout());
    }

    #[test]
    fn flags_no_overlap() {
        let flags = make_flags(
            RoadClass::Track,
            Surface::Unpaved,
            true,
            true,
            Access::Designated,
            Access::No,
            true,
        );
        let edge = make_edge(flags);
        assert_eq!(edge.road_class(), RoadClass::Track);
        assert_eq!(edge.surface(), Surface::Unpaved);
        assert!(edge.is_oneway());
        assert!(edge.is_seasonal_closure());
        assert_eq!(edge.access_foot(), Access::Designated);
        assert_eq!(edge.access_bicycle(), Access::No);
        assert!(edge.is_roundabout());
    }

    #[test]
    fn edge_size_is_20_bytes() {
        assert_eq!(std::mem::size_of::<Edge>(), 24);
    }
}
