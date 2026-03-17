use std::path::Path;
use std::sync::Mutex;

use atlas_core::bbox::AFRICA;
use atlas_core::{Address, Category, Lang, OsmId, Place, PlaceId, Source};
use osmpbf::{Element, ElementReader};
use tracing::warn;

pub fn read_osm_places(dir: &Path) -> Result<Vec<Place>, Box<dyn std::error::Error>> {
    if !dir.exists() {
        warn!("OSM directory does not exist: {}", dir.display());
        return Ok(vec![]);
    }

    let pbf_files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.to_string_lossy();
            name.ends_with(".osm.pbf")
        })
        .collect();

    if pbf_files.is_empty() {
        warn!("No .osm.pbf files found in: {}", dir.display());
        return Ok(vec![]);
    }

    let mut all_places = Vec::new();

    for path in pbf_files {
        let reader = ElementReader::from_path(&path)?;
        let places_mutex = Mutex::new(Vec::new());

        reader.for_each(|element| {
            if let Some(place) = extract_place_from_element(&element) {
                if let Ok(mut guard) = places_mutex.lock() {
                    guard.push(place);
                }
            }
        })?;

        let collected = places_mutex.into_inner()?;
        all_places.extend(collected);
    }

    Ok(all_places)
}

fn extract_place_from_element(element: &Element<'_>) -> Option<Place> {
    match element {
        Element::Node(node) => {
            let tags: Vec<(&str, &str)> = node.tags().collect();
            let name = find_tag(&tags, "name")?;
            if name.is_empty() {
                return None;
            }

            let lat = node.lat();
            let lon = node.lon();
            if !AFRICA.contains(lon, lat) {
                return None;
            }

            let category = extract_category(&tags)?;
            let names = extract_names(&tags);
            let address = extract_address(&tags);

            Some(Place {
                id: PlaceId::Osm(OsmId::Node(node.id())),
                names,
                category,
                lat,
                lon,
                address,
                source: Source::Osm,
            })
        }
        Element::DenseNode(node) => {
            let tags: Vec<(&str, &str)> = node.tags().collect();
            let name = find_tag(&tags, "name")?;
            if name.is_empty() {
                return None;
            }

            let lat = node.lat();
            let lon = node.lon();
            if !AFRICA.contains(lon, lat) {
                return None;
            }

            let category = extract_category(&tags)?;
            let names = extract_names(&tags);
            let address = extract_address(&tags);

            Some(Place {
                id: PlaceId::Osm(OsmId::Node(node.id())),
                names,
                category,
                lat,
                lon,
                address,
                source: Source::Osm,
            })
        }
        Element::Way(_) => {
            // Ways don't carry coordinates in osmpbf — node references would need
            // a second pass to resolve. Skip for now; most POIs are nodes.
            None
        }
        Element::Relation(_) => None,
    }
}

fn find_tag<'a>(tags: &[(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    tags.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn extract_category(tags: &[(&str, &str)]) -> Option<Category> {
    let candidates = [
        find_tag(tags, "amenity"),
        find_tag(tags, "shop"),
        find_tag(tags, "tourism"),
        find_tag(tags, "building"),
        find_tag(tags, "place"),
        find_tag(tags, "landuse"),
    ];

    for candidate in candidates.into_iter().flatten() {
        if let Some(cat) = map_osm_tag_to_category(candidate) {
            return Some(cat);
        }
    }

    None
}

fn map_osm_tag_to_category(value: &str) -> Option<Category> {
    match value {
        "marketplace" | "market" => Some(Category::Market),
        "mosque" | "masjid" => Some(Category::Mosque),
        "church" | "chapel" | "cathedral" | "place_of_worship" => Some(Category::Church),
        "school" | "kindergarten" => Some(Category::School),
        "university" | "college" => Some(Category::University),
        "hospital" | "clinic" | "doctors" | "pharmacy" | "health_centre" => {
            Some(Category::Hospital)
        }
        "fuel" => Some(Category::FuelStation),
        "tower" | "mast" | "communication_tower" => Some(Category::TelecomTower),
        "bank" | "atm" => Some(Category::Bank),
        "restaurant" | "cafe" | "fast_food" | "food_court" | "bar" => Some(Category::Restaurant),
        "hotel" | "motel" | "hostel" | "guest_house" => Some(Category::Hotel),
        "bus_station" | "train_station" | "ferry_terminal" | "bus_stop" | "station" => {
            Some(Category::TransportStop)
        }
        "townhall" | "courthouse" | "government" => Some(Category::Government),
        "residential" | "apartments" | "house" => Some(Category::Residential),
        "supermarket" | "mall" | "convenience" | "commercial" | "retail" => {
            Some(Category::Commercial)
        }
        _ => None,
    }
}

fn extract_names(tags: &[(&str, &str)]) -> Vec<(Lang, String)> {
    let mut names = Vec::new();

    let lang_keys: &[(&str, Lang)] = &[
        ("name:en", Lang::En),
        ("name:fr", Lang::Fr),
        ("name:ar", Lang::Ar),
        ("name:sw", Lang::Sw),
    ];

    for (key, lang) in lang_keys {
        if let Some(val) = find_tag(tags, key) {
            if !val.is_empty() {
                names.push((lang.clone(), val.to_string()));
            }
        }
    }

    if let Some(default_name) = find_tag(tags, "name") {
        if !default_name.is_empty() {
            let already_has = names.iter().any(|(_, n)| n == default_name);
            if !already_has {
                names.insert(0, (Lang::En, default_name.to_string()));
            }
        }
    }

    names
}

fn extract_address(tags: &[(&str, &str)]) -> Option<Address> {
    let street = find_tag(tags, "addr:street").map(str::to_string);
    let city = find_tag(tags, "addr:city").map(str::to_string);
    let region = find_tag(tags, "addr:state").map(str::to_string);
    let postcode = find_tag(tags, "addr:postcode").map(str::to_string);
    let country = find_tag(tags, "addr:country")
        .map(str::to_string)
        .unwrap_or_default();

    if street.is_none() && city.is_none() && country.is_empty() {
        return None;
    }

    Some(Address {
        street,
        city,
        region,
        postcode,
        country,
    })
}
