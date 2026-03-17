use std::path::Path;

use atlas_core::{AtlasError, Place};
use atlas_geocode::tokenizer::{strip_diacritics, AsciiFolder, AtlasTokenizer};
use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST, STORED, STRING, TEXT,
};
use tantivy::{Index, IndexReader, TantivyDocument, Term};

struct Fields {
    name: Field,
    name_ascii: Field,
    category: Field,
    country: Field,
    city: Field,
    lat: Field,
    lon: Field,
    popularity: Field,
    address_full: Field,
}

fn build_schema() -> Schema {
    let mut builder = Schema::builder();

    let atlas_text_opts = TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("atlas")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );

    let ascii_text_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("atlas_ascii")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );

    builder.add_text_field("name", atlas_text_opts);
    builder.add_text_field("name_ascii", ascii_text_opts);
    builder.add_text_field("category", STRING | STORED);
    builder.add_text_field("country", STRING | STORED);
    builder.add_text_field("city", TEXT | STORED);
    builder.add_f64_field("lat", FAST | STORED);
    builder.add_f64_field("lon", FAST | STORED);
    builder.add_u64_field("popularity", FAST | STORED);
    builder.add_text_field("address_full", TEXT | STORED);

    builder.build()
}

fn get_fields(schema: &Schema) -> Fields {
    Fields {
        name: schema.get_field("name").expect("name field missing"),
        name_ascii: schema
            .get_field("name_ascii")
            .expect("name_ascii field missing"),
        category: schema
            .get_field("category")
            .expect("category field missing"),
        country: schema.get_field("country").expect("country field missing"),
        city: schema.get_field("city").expect("city field missing"),
        lat: schema.get_field("lat").expect("lat field missing"),
        lon: schema.get_field("lon").expect("lon field missing"),
        popularity: schema
            .get_field("popularity")
            .expect("popularity field missing"),
        address_full: schema
            .get_field("address_full")
            .expect("address_full field missing"),
    }
}

fn register_tokenizers(index: &Index) {
    index.tokenizers().register("atlas", AtlasTokenizer);
    index.tokenizers().register("atlas_ascii", AsciiFolder);
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let a =
        (dlat / 2.0).sin().powi(2) + lat1_rad.cos() * lat2_rad.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_RADIUS_KM * c
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub category: String,
    pub address: Option<String>,
    pub distance_m: Option<f64>,
    pub score: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SearchOpts {
    pub limit: usize,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub radius_km: Option<f64>,
    pub category: Option<String>,
    pub country: Option<String>,
}

pub struct SearchEngine {
    schema: Schema,
    index: Index,
    reader: IndexReader,
}

impl SearchEngine {
    pub fn build(places: &[Place], output_dir: &Path) -> Result<(), AtlasError> {
        std::fs::create_dir_all(output_dir)
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;

        let schema = build_schema();
        let index = Index::create_in_dir(output_dir, schema.clone())
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;

        register_tokenizers(&index);

        let fields = get_fields(&schema);
        let mut writer: tantivy::IndexWriter<TantivyDocument> = index
            .writer(50_000_000)
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;

        for place in places {
            let primary_name = place.primary_name(None).to_string();
            let ascii_name = strip_diacritics(&primary_name);

            let country = place
                .address
                .as_ref()
                .map(|a| a.country.clone())
                .unwrap_or_default();

            let city = place
                .address
                .as_ref()
                .and_then(|a| a.city.clone())
                .unwrap_or_default();

            let address_full = place
                .address
                .as_ref()
                .map(|a| a.full_string())
                .unwrap_or_default();

            let popularity = place.names.len() as u64;

            let mut doc = TantivyDocument::new();
            doc.add_text(fields.name, &primary_name);
            doc.add_text(fields.name_ascii, &ascii_name);
            doc.add_text(fields.category, place.category.as_str());
            doc.add_text(fields.country, &country);
            doc.add_text(fields.city, &city);
            doc.add_f64(fields.lat, place.lat);
            doc.add_f64(fields.lon, place.lon);
            doc.add_u64(fields.popularity, popularity);
            doc.add_text(fields.address_full, &address_full);

            writer
                .add_document(doc)
                .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;
        }

        writer
            .commit()
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;

        Ok(())
    }

    pub fn open(index_dir: &Path) -> Result<Self, AtlasError> {
        let index = Index::open_in_dir(index_dir)
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;

        register_tokenizers(&index);

        let schema = index.schema();
        let reader = index
            .reader()
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;

        Ok(SearchEngine {
            schema,
            index,
            reader,
        })
    }

    pub fn search(
        &self,
        query_text: &str,
        opts: &SearchOpts,
    ) -> Result<Vec<SearchResult>, AtlasError> {
        let limit = if opts.limit == 0 { 10 } else { opts.limit };

        if query_text.trim().is_empty() && opts.category.is_none() {
            return Ok(vec![]);
        }

        let fields = get_fields(&self.schema);
        let searcher = self.reader.searcher();

        let fetch_limit = limit * 3;

        let search_query: Box<dyn tantivy::query::Query> = if query_text.trim().is_empty() {
            let category = opts.category.as_deref().unwrap_or("");
            let term = Term::from_field_text(fields.category, category);
            Box::new(TermQuery::new(term, IndexRecordOption::Basic))
        } else {
            let mut parser =
                QueryParser::for_index(&self.index, vec![fields.name, fields.name_ascii]);
            parser.set_conjunction_by_default();

            let base_query = parser
                .parse_query(query_text)
                .map_err(|e| AtlasError::QueryParseError(e.to_string()))?;

            if let Some(ref category) = opts.category {
                let cat_term = Term::from_field_text(fields.category, category.as_str());
                let cat_query: Box<dyn tantivy::query::Query> =
                    Box::new(TermQuery::new(cat_term, IndexRecordOption::Basic));
                Box::new(BooleanQuery::new(vec![
                    (Occur::Must, base_query),
                    (Occur::Must, cat_query),
                ]))
            } else {
                base_query
            }
        };

        let final_query: Box<dyn tantivy::query::Query> = if let Some(ref country) = opts.country {
            let country_term = Term::from_field_text(fields.country, country.as_str());
            let country_query: Box<dyn tantivy::query::Query> =
                Box::new(TermQuery::new(country_term, IndexRecordOption::Basic));
            Box::new(BooleanQuery::new(vec![
                (Occur::Must, search_query),
                (Occur::Must, country_query),
            ]))
        } else {
            search_query
        };

        let top_docs = searcher
            .search(&final_query, &TopDocs::with_limit(fetch_limit))
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;

        if top_docs.is_empty() {
            return Ok(vec![]);
        }

        let max_bm25 = top_docs[0].0;

        let mut scored: Vec<SearchResult> = Vec::with_capacity(top_docs.len());

        for (bm25_score, doc_address) in &top_docs {
            let doc: TantivyDocument = searcher
                .doc(*doc_address)
                .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;

            let name = extract_str(&doc, fields.name);
            let lat = extract_f64(&doc, fields.lat);
            let lon = extract_f64(&doc, fields.lon);
            let category = extract_str(&doc, fields.category);
            let popularity = extract_u64(&doc, fields.popularity);

            let address_raw = extract_str(&doc, fields.address_full);
            let address = if address_raw.is_empty() {
                None
            } else {
                Some(address_raw)
            };

            let (distance_km, distance_m) = match (opts.lat, opts.lon) {
                (Some(user_lat), Some(user_lon)) => {
                    let dk = haversine_km(user_lat, user_lon, lat, lon);
                    (Some(dk), Some(dk * 1000.0))
                }
                _ => (None, None),
            };

            if let Some(radius) = opts.radius_km {
                if let Some(dk) = distance_km {
                    if dk > radius {
                        continue;
                    }
                }
            }

            let bm25_norm = if max_bm25 > 0.0 {
                *bm25_score as f64 / max_bm25 as f64
            } else {
                0.0
            };

            let distance_factor = match distance_km {
                Some(dk) => 1.0 / (1.0 + dk),
                None => 0.0,
            };

            let popularity_boost = (popularity as f64).log2().max(0.0) / 20.0;

            let category_bonus = if opts.category.as_deref() == Some(category.as_str()) {
                0.1_f64
            } else {
                0.0
            };

            let score = (0.4 * bm25_norm)
                + (0.4 * distance_factor)
                + (0.1 * popularity_boost)
                + (0.1 * category_bonus);

            scored.push(SearchResult {
                name,
                lat,
                lon,
                category,
                address,
                distance_m,
                score,
            });
        }

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);

        Ok(scored)
    }

    pub fn autocomplete(
        &self,
        prefix: &str,
        opts: &SearchOpts,
    ) -> Result<Vec<SearchResult>, AtlasError> {
        if prefix.trim().is_empty() {
            return Ok(vec![]);
        }
        self.search(prefix, opts)
    }
}

fn extract_str(doc: &TantivyDocument, field: Field) -> String {
    doc.get_first(field)
        .and_then(|v| {
            if let tantivy::schema::OwnedValue::Str(s) = v {
                Some(s.clone())
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn extract_f64(doc: &TantivyDocument, field: Field) -> f64 {
    doc.get_first(field)
        .and_then(|v| {
            if let tantivy::schema::OwnedValue::F64(f) = v {
                Some(*f)
            } else {
                None
            }
        })
        .unwrap_or(0.0)
}

fn extract_u64(doc: &TantivyDocument, field: Field) -> u64 {
    doc.get_first(field)
        .and_then(|v| {
            if let tantivy::schema::OwnedValue::U64(u) = v {
                Some(*u)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::{Address, Category, Lang, OsmId, Place, PlaceId, Source};

    fn make_place(name: &str, category: Category, lat: f64, lon: f64, country: &str) -> Place {
        Place {
            id: PlaceId::Osm(OsmId::Node(1)),
            names: vec![(Lang::En, name.to_string())],
            category,
            lat,
            lon,
            address: Some(Address {
                street: None,
                city: Some("TestCity".to_string()),
                region: None,
                postcode: None,
                country: country.to_string(),
            }),
            source: Source::Osm,
        }
    }

    fn make_place_multi_name(
        name: &str,
        names: Vec<(Lang, String)>,
        category: Category,
        lat: f64,
        lon: f64,
        country: &str,
    ) -> Place {
        let _ = name;
        Place {
            id: PlaceId::Osm(OsmId::Node(1)),
            names,
            category,
            lat,
            lon,
            address: Some(Address {
                street: None,
                city: Some("TestCity".to_string()),
                region: None,
                postcode: None,
                country: country.to_string(),
            }),
            source: Source::Osm,
        }
    }

    fn build_test_engine(places: &[Place]) -> (SearchEngine, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        SearchEngine::build(places, dir.path()).unwrap();
        let engine = SearchEngine::open(dir.path()).unwrap();
        (engine, dir)
    }

    #[test]
    fn search_by_name_finds_place() {
        let places = vec![make_place(
            "Makola Market",
            Category::Market,
            5.55,
            -0.21,
            "Ghana",
        )];
        let (engine, _dir) = build_test_engine(&places);
        let opts = SearchOpts::default();
        let results = engine.search("Makola", &opts).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].name.contains("Makola"));
    }

    #[test]
    fn distance_scoring_nearby_ranks_higher() {
        let places = vec![
            make_place(
                "Test Hospital Near",
                Category::Hospital,
                5.55,
                -0.21,
                "Ghana",
            ),
            make_place("Test Hospital Far", Category::Hospital, 10.0, 20.0, "Ghana"),
        ];
        let (engine, _dir) = build_test_engine(&places);
        let opts = SearchOpts {
            limit: 10,
            lat: Some(5.55),
            lon: Some(-0.21),
            radius_km: None,
            category: None,
            country: None,
        };
        let results = engine.search("Test Hospital", &opts).unwrap();
        assert!(results.len() >= 2);
        assert!(
            results[0].name.contains("Near"),
            "nearby should rank first, got: {}",
            results[0].name
        );
    }

    #[test]
    fn category_filter_restricts_results() {
        let places = vec![
            make_place(
                "Accra Central Mosque",
                Category::Mosque,
                5.55,
                -0.21,
                "Ghana",
            ),
            make_place("Accra Central Bank", Category::Bank, 5.56, -0.20, "Ghana"),
        ];
        let (engine, _dir) = build_test_engine(&places);
        let opts = SearchOpts {
            limit: 10,
            lat: None,
            lon: None,
            radius_km: None,
            category: Some("mosque".to_string()),
            country: None,
        };
        let results = engine.search("Accra Central", &opts).unwrap();
        assert!(!results.is_empty());
        for result in &results {
            assert_eq!(result.category, "mosque");
        }
    }

    #[test]
    fn empty_query_no_category_returns_empty() {
        let places = vec![make_place("Somewhere", Category::Market, 0.0, 0.0, "KE")];
        let (engine, _dir) = build_test_engine(&places);
        let opts = SearchOpts::default();
        let results = engine.search("", &opts).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn radius_filter_excludes_far_places() {
        let places = vec![
            make_place("Near Place", Category::Hospital, 5.55, -0.21, "Ghana"),
            make_place("Far Place", Category::Hospital, 30.0, 31.0, "Kenya"),
        ];
        let (engine, _dir) = build_test_engine(&places);
        let opts = SearchOpts {
            limit: 10,
            lat: Some(5.55),
            lon: Some(-0.21),
            radius_km: Some(5.0),
            category: None,
            country: None,
        };
        let results = engine.search("Place", &opts).unwrap();
        assert!(!results.is_empty());
        for result in &results {
            assert!(
                result.distance_m.unwrap() <= 5000.0,
                "expected within 5km, got {}m",
                result.distance_m.unwrap()
            );
        }
    }

    #[test]
    fn popularity_uses_names_len() {
        let popular = make_place_multi_name(
            "Popular Market",
            vec![
                (Lang::En, "Popular Market".to_string()),
                (Lang::Fr, "Marché Populaire".to_string()),
                (Lang::Ar, "السوق الشعبي".to_string()),
            ],
            Category::Market,
            5.55,
            -0.21,
            "Ghana",
        );
        let places = vec![popular];
        let dir = tempfile::tempdir().unwrap();
        SearchEngine::build(&places, dir.path()).unwrap();
        let engine = SearchEngine::open(dir.path()).unwrap();
        let opts = SearchOpts::default();
        let results = engine.search("Popular Market", &opts).unwrap();
        assert!(!results.is_empty());
    }
}
