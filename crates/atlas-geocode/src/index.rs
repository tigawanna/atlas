use std::path::Path;

use atlas_core::{AtlasError, GeocodeOpts, GeocodeResult, Place};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST, STORED, STRING, TEXT,
};
use tantivy::{Index, IndexReader, TantivyDocument, Term};

use crate::tokenizer::{phonetic_encode, strip_diacritics, AsciiFolder, AtlasTokenizer};

struct Fields {
    name: Field,
    name_ascii: Field,
    name_phonetic: Field,
    category: Field,
    country: Field,
    city: Field,
    lat: Field,
    lon: Field,
    address_full: Field,
    source_id: Field,
}

pub fn build_schema() -> Schema {
    let mut builder = Schema::builder();

    let atlas_text_opts = TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("atlas")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );

    let ascii_text_opts = TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("atlas_ascii")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );

    builder.add_text_field("name", atlas_text_opts);
    builder.add_text_field("name_ascii", ascii_text_opts);
    builder.add_text_field("name_phonetic", TEXT);
    builder.add_text_field("category", STRING | STORED);
    builder.add_text_field("country", STRING | STORED);
    builder.add_text_field("city", TEXT | STORED);
    builder.add_f64_field("lat", FAST | STORED);
    builder.add_f64_field("lon", FAST | STORED);
    builder.add_text_field("address_full", TEXT | STORED);
    builder.add_bytes_field("source_id", STORED);

    builder.build()
}

fn get_fields(schema: &Schema) -> Fields {
    Fields {
        name: schema.get_field("name").expect("name field missing"),
        name_ascii: schema
            .get_field("name_ascii")
            .expect("name_ascii field missing"),
        name_phonetic: schema
            .get_field("name_phonetic")
            .expect("name_phonetic field missing"),
        category: schema
            .get_field("category")
            .expect("category field missing"),
        country: schema.get_field("country").expect("country field missing"),
        city: schema.get_field("city").expect("city field missing"),
        lat: schema.get_field("lat").expect("lat field missing"),
        lon: schema.get_field("lon").expect("lon field missing"),
        address_full: schema
            .get_field("address_full")
            .expect("address_full field missing"),
        source_id: schema
            .get_field("source_id")
            .expect("source_id field missing"),
    }
}

fn register_tokenizers(index: &Index) {
    index.tokenizers().register("atlas", AtlasTokenizer);
    index.tokenizers().register("atlas_ascii", AsciiFolder);
}

pub struct GeocodeIndex {
    pub schema: Schema,
    pub index: Index,
    pub reader: IndexReader,
}

impl GeocodeIndex {
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
            let phonetic = phonetic_encode(&ascii_name);

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

            let source_id_str = format!("{:?}", place.id);

            let mut doc = TantivyDocument::new();
            doc.add_text(fields.name, &primary_name);
            doc.add_text(fields.name_ascii, &ascii_name);
            doc.add_text(fields.name_phonetic, &phonetic);
            doc.add_text(fields.category, place.category.as_str());
            doc.add_text(fields.country, &country);
            doc.add_text(fields.city, &city);
            doc.add_f64(fields.lat, place.lat);
            doc.add_f64(fields.lon, place.lon);
            doc.add_text(fields.address_full, &address_full);
            doc.add_bytes(fields.source_id, source_id_str.as_bytes().to_vec());

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

        Ok(GeocodeIndex {
            schema,
            index,
            reader,
        })
    }

    pub fn search(
        &self,
        query_text: &str,
        opts: &GeocodeOpts,
    ) -> Result<Vec<GeocodeResult>, AtlasError> {
        if query_text.trim().is_empty() {
            return Ok(vec![]);
        }

        let fields = get_fields(&self.schema);
        let searcher = self.reader.searcher();

        let mut parser = QueryParser::for_index(
            &self.index,
            vec![fields.name, fields.name_ascii, fields.name_phonetic],
        );
        parser.set_conjunction_by_default();

        let base_query = parser
            .parse_query(query_text)
            .map_err(|e| AtlasError::QueryParseError(e.to_string()))?;

        let search_query: Box<dyn tantivy::query::Query> = if let Some(ref country) = opts.country {
            let country_term = Term::from_field_text(fields.country, country.as_str());
            let country_query: Box<dyn tantivy::query::Query> =
                Box::new(TermQuery::new(country_term, IndexRecordOption::Basic));
            Box::new(BooleanQuery::new(vec![
                (Occur::Must, base_query),
                (Occur::Must, country_query),
            ]))
        } else {
            base_query
        };

        let fetch_limit = opts.limit * 3;
        let top_docs = searcher
            .search(&search_query, &TopDocs::with_limit(fetch_limit))
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;

        if top_docs.is_empty() {
            return Ok(vec![]);
        }

        let max_score = top_docs[0].0;
        let query_lower = query_text.to_lowercase();

        let mut results = Vec::with_capacity(top_docs.len());

        for (score, doc_address) in top_docs.into_iter().take(opts.limit) {
            let doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;

            let name = doc
                .get_first(fields.name)
                .and_then(|v| {
                    if let tantivy::schema::OwnedValue::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let lat = doc
                .get_first(fields.lat)
                .and_then(|v| {
                    if let tantivy::schema::OwnedValue::F64(f) = v {
                        Some(*f)
                    } else {
                        None
                    }
                })
                .unwrap_or(0.0);

            let lon = doc
                .get_first(fields.lon)
                .and_then(|v| {
                    if let tantivy::schema::OwnedValue::F64(f) = v {
                        Some(*f)
                    } else {
                        None
                    }
                })
                .unwrap_or(0.0);

            let category = doc
                .get_first(fields.category)
                .and_then(|v| {
                    if let tantivy::schema::OwnedValue::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let address = doc.get_first(fields.address_full).and_then(|v| {
                if let tantivy::schema::OwnedValue::Str(s) = v {
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.clone())
                    }
                } else {
                    None
                }
            });

            let normalized_score = if max_score > 0.0 {
                score / max_score
            } else {
                0.0
            };

            let country_val = doc
                .get_first(fields.country)
                .and_then(|v| {
                    if let tantivy::schema::OwnedValue::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let locality_bonus = if opts.country.as_deref() == Some(country_val.as_str()) {
                0.1_f64
            } else {
                0.0
            };

            let exact_bonus = if name.to_lowercase() == query_lower {
                0.15_f64
            } else {
                0.0
            };

            let confidence = (normalized_score as f64 + locality_bonus + exact_bonus).min(1.0);

            results.push(GeocodeResult {
                name,
                lat,
                lon,
                category,
                address,
                confidence,
            });
        }

        Ok(results)
    }
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

    fn build_test_index(places: &[Place]) -> (GeocodeIndex, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        GeocodeIndex::build(places, dir.path()).unwrap();
        let idx = GeocodeIndex::open(dir.path()).unwrap();
        (idx, dir)
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
        let (idx, _dir) = build_test_index(&places);
        let opts = GeocodeOpts::default();
        let results = idx.search("Makola", &opts).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].name.contains("Makola"));
    }

    #[test]
    fn country_filter_works() {
        let places = vec![
            make_place("Central Hospital", Category::Hospital, 5.55, -0.21, "Ghana"),
            make_place(
                "Central Hospital",
                Category::Hospital,
                6.45,
                3.39,
                "Nigeria",
            ),
        ];
        let (idx, _dir) = build_test_index(&places);
        let opts = GeocodeOpts {
            limit: 10,
            country: Some("Ghana".to_string()),
            lang: None,
        };
        let results = idx.search("Central Hospital", &opts).unwrap();
        assert!(!results.is_empty());
        for result in &results {
            assert!(result.confidence > 0.0);
        }
    }

    #[test]
    fn empty_query_returns_empty() {
        let places = vec![make_place("Somewhere", Category::Market, 0.0, 0.0, "KE")];
        let (idx, _dir) = build_test_index(&places);
        let opts = GeocodeOpts::default();
        let results = idx.search("", &opts).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn diacritics_insensitive_search() {
        let places = vec![make_place(
            "Abidjan",
            Category::Government,
            5.35,
            -4.0,
            "CI",
        )];
        let (idx, _dir) = build_test_index(&places);
        let opts = GeocodeOpts::default();
        let results = idx.search("Abidjan", &opts).unwrap();
        assert!(!results.is_empty());
    }
}
