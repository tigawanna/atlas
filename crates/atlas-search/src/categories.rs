pub const SEARCH_CATEGORIES: &[(&str, &[&str])] = &[
    (
        "restaurant",
        &["restaurant", "cafe", "fast_food", "food_court"],
    ),
    ("hotel", &["hotel", "motel", "guest_house", "hostel"]),
    ("hospital", &["hospital", "clinic", "doctors", "pharmacy"]),
    ("bank", &["bank", "atm"]),
    ("mosque", &["mosque"]),
    ("church", &["church", "chapel", "cathedral"]),
    ("market", &["market", "marketplace", "supermarket"]),
    ("fuel", &["fuel_station", "gas_station"]),
    ("school", &["school", "university", "college"]),
    ("transport", &["bus_stop", "bus_station", "taxi", "trotro"]),
];

pub fn resolve_category(user_input: &str) -> Vec<&'static str> {
    let lower = user_input.to_lowercase();
    for &(name, variants) in SEARCH_CATEGORIES {
        if name == lower {
            return variants.to_vec();
        }
    }
    vec![]
}
