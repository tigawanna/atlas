use rstar::{RTreeObject, AABB};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlaceId {
    Overture(String),
    Osm(OsmId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OsmId {
    Node(i64),
    Way(i64),
    Relation(i64),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Lang {
    En,
    Fr,
    Ar,
    Sw,
    Tw,
    Yo,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Overture,
    Osm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    Market,
    Mosque,
    Church,
    School,
    University,
    Hospital,
    FuelStation,
    TelecomTower,
    Bank,
    Restaurant,
    Hotel,
    TransportStop,
    Government,
    Residential,
    Commercial,
}

impl Category {
    pub fn as_str(&self) -> &'static str {
        match self {
            Category::Market => "market",
            Category::Mosque => "mosque",
            Category::Church => "church",
            Category::School => "school",
            Category::University => "university",
            Category::Hospital => "hospital",
            Category::FuelStation => "fuel_station",
            Category::TelecomTower => "telecom_tower",
            Category::Bank => "bank",
            Category::Restaurant => "restaurant",
            Category::Hotel => "hotel",
            Category::TransportStop => "transport_stop",
            Category::Government => "government",
            Category::Residential => "residential",
            Category::Commercial => "commercial",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "market" | "marketplace" | "bazaar" | "souk" => Some(Category::Market),
            "mosque" | "masjid" => Some(Category::Mosque),
            "church" | "chapel" | "cathedral" => Some(Category::Church),
            "school" | "primary_school" | "secondary_school" => Some(Category::School),
            "university" | "college" => Some(Category::University),
            "hospital" | "clinic" | "health_centre" | "health_center" => Some(Category::Hospital),
            "fuel_station" | "fuel" | "petrol_station" | "gas_station" => {
                Some(Category::FuelStation)
            }
            "telecom_tower" | "tower" | "mast" | "communication_tower" => {
                Some(Category::TelecomTower)
            }
            "bank" | "atm" => Some(Category::Bank),
            "restaurant" | "cafe" | "fast_food" | "food_court" => Some(Category::Restaurant),
            "hotel" | "motel" | "guesthouse" | "hostel" => Some(Category::Hotel),
            "transport_stop" | "bus_stop" | "train_station" | "station" | "ferry_terminal" => {
                Some(Category::TransportStop)
            }
            "government" | "government_office" | "town_hall" | "courthouse" => {
                Some(Category::Government)
            }
            "residential" | "house" | "apartment" => Some(Category::Residential),
            "commercial" | "shop" | "mall" | "supermarket" => Some(Category::Commercial),
            _ => None,
        }
    }

    pub fn is_landmark(&self) -> bool {
        matches!(
            self,
            Category::Mosque
                | Category::Church
                | Category::University
                | Category::Hospital
                | Category::TelecomTower
                | Category::Government
                | Category::TransportStop
                | Category::Market
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub street: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postcode: Option<String>,
    pub country: String,
}

impl Address {
    pub fn full_string(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if let Some(ref s) = self.street {
            parts.push(s.as_str());
        }
        if let Some(ref c) = self.city {
            parts.push(c.as_str());
        }
        if let Some(ref r) = self.region {
            parts.push(r.as_str());
        }
        if let Some(ref p) = self.postcode {
            parts.push(p.as_str());
        }
        parts.push(self.country.as_str());
        parts.join(", ")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
    pub id: PlaceId,
    pub names: Vec<(Lang, String)>,
    pub category: Category,
    pub lat: f64,
    pub lon: f64,
    pub address: Option<Address>,
    pub source: Source,
}

impl Place {
    pub fn primary_name(&self, preferred_lang: Option<&Lang>) -> &str {
        if let Some(lang) = preferred_lang {
            if let Some((_, name)) = self.names.iter().find(|(l, _)| l == lang) {
                return name.as_str();
            }
        }
        self.names
            .first()
            .map(|(_, name)| name.as_str())
            .unwrap_or("")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacePoint {
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub category: Category,
    pub address_summary: Option<String>,
    pub names: Vec<(Lang, String)>,
}

impl RTreeObject for PlacePoint {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point([self.lon, self.lat])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandmarkPoint {
    pub lat: f64,
    pub lon: f64,
    pub names: Vec<(Lang, String)>,
    pub category: Category,
}

impl RTreeObject for LandmarkPoint {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point([self.lon, self.lat])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_place(names: Vec<(Lang, String)>) -> Place {
        Place {
            id: PlaceId::Osm(OsmId::Node(1)),
            names,
            category: Category::Market,
            lat: 5.0,
            lon: 1.0,
            address: None,
            source: Source::Osm,
        }
    }

    #[test]
    fn primary_name_preferred_lang() {
        let place = make_place(vec![
            (Lang::En, "English Name".to_string()),
            (Lang::Fr, "French Name".to_string()),
        ]);
        assert_eq!(place.primary_name(Some(&Lang::Fr)), "French Name");
    }

    #[test]
    fn primary_name_fallback_to_first() {
        let place = make_place(vec![
            (Lang::En, "English Name".to_string()),
            (Lang::Fr, "French Name".to_string()),
        ]);
        assert_eq!(place.primary_name(Some(&Lang::Ar)), "English Name");
    }

    #[test]
    fn primary_name_no_preferred_lang() {
        let place = make_place(vec![(Lang::Sw, "Swahili Name".to_string())]);
        assert_eq!(place.primary_name(None), "Swahili Name");
    }

    #[test]
    fn primary_name_empty_names() {
        let place = make_place(vec![]);
        assert_eq!(place.primary_name(None), "");
    }

    #[test]
    fn category_from_str_opt_known() {
        assert_eq!(
            Category::from_str_opt("marketplace"),
            Some(Category::Market)
        );
        assert_eq!(Category::from_str_opt("mosque"), Some(Category::Mosque));
        assert_eq!(Category::from_str_opt("masjid"), Some(Category::Mosque));
        assert_eq!(Category::from_str_opt("hospital"), Some(Category::Hospital));
        assert_eq!(
            Category::from_str_opt("bus_stop"),
            Some(Category::TransportStop)
        );
        assert_eq!(
            Category::from_str_opt("university"),
            Some(Category::University)
        );
    }

    #[test]
    fn category_from_str_opt_unknown() {
        assert_eq!(Category::from_str_opt("unknown_type"), None);
    }

    #[test]
    fn category_is_landmark_true() {
        assert!(Category::Mosque.is_landmark());
        assert!(Category::Hospital.is_landmark());
        assert!(Category::University.is_landmark());
        assert!(Category::TelecomTower.is_landmark());
        assert!(Category::Government.is_landmark());
        assert!(Category::Market.is_landmark());
    }

    #[test]
    fn category_is_landmark_false() {
        assert!(!Category::Restaurant.is_landmark());
        assert!(!Category::Hotel.is_landmark());
        assert!(!Category::Residential.is_landmark());
        assert!(!Category::Commercial.is_landmark());
        assert!(!Category::Bank.is_landmark());
    }

    #[test]
    fn address_full_string_all_fields() {
        let addr = Address {
            street: Some("123 Main St".to_string()),
            city: Some("Accra".to_string()),
            region: Some("Greater Accra".to_string()),
            postcode: Some("00233".to_string()),
            country: "Ghana".to_string(),
        };
        assert_eq!(
            addr.full_string(),
            "123 Main St, Accra, Greater Accra, 00233, Ghana"
        );
    }

    #[test]
    fn address_full_string_partial_fields() {
        let addr = Address {
            street: None,
            city: Some("Lagos".to_string()),
            region: None,
            postcode: None,
            country: "Nigeria".to_string(),
        };
        assert_eq!(addr.full_string(), "Lagos, Nigeria");
    }

    #[test]
    fn address_full_string_country_only() {
        let addr = Address {
            street: None,
            city: None,
            region: None,
            postcode: None,
            country: "Kenya".to_string(),
        };
        assert_eq!(addr.full_string(), "Kenya");
    }

    #[test]
    fn place_point_rtree_envelope() {
        use rstar::RTreeObject;
        let point = PlacePoint {
            lat: 5.6037,
            lon: -0.1870,
            name: "Makola Market".to_string(),
            category: Category::Market,
            address_summary: None,
            names: vec![(Lang::En, "Makola Market".to_string())],
        };
        let envelope = point.envelope();
        let lower = envelope.lower();
        let upper = envelope.upper();
        assert!((lower[0] - (-0.1870_f64)).abs() < f64::EPSILON);
        assert!((lower[1] - 5.6037_f64).abs() < f64::EPSILON);
        assert!((upper[0] - (-0.1870_f64)).abs() < f64::EPSILON);
        assert!((upper[1] - 5.6037_f64).abs() < f64::EPSILON);
    }
}
