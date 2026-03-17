pub mod africa;
pub mod bicycle;
pub mod car;
pub mod foot;
pub mod motorcycle;

use crate::graph::Edge;

pub trait RoutingProfile: Send + Sync {
    fn name(&self) -> &str;
    fn edge_weight(&self, edge: &Edge) -> Option<u32>;
    fn is_accessible(&self, edge: &Edge) -> bool;
}

pub use bicycle::BicycleProfile;
pub use car::CarProfile;
pub use foot::FootProfile;
pub use motorcycle::MotorcycleProfile;
