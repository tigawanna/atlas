use std::path::Path;

use arrow::array::{Array, Float64Array, StringArray, StructArray};
use arrow::datatypes::DataType;
use atlas_core::bbox::AFRICA;
use atlas_core::{Address, Category, Lang, Place, PlaceId, Source};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use tracing::warn;

pub fn read_overture_places(dir: &Path) -> Result<Vec<Place>, Box<dyn std::error::Error>> {
    if !dir.exists() {
        warn!("Overture directory does not exist: {}", dir.display());
        return Ok(vec![]);
    }

    let parquet_files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("parquet"))
        .collect();

    if parquet_files.is_empty() {
        warn!("No parquet files found in: {}", dir.display());
        return Ok(vec![]);
    }

    let mut places = Vec::new();

    for path in parquet_files {
        let file = std::fs::File::open(&path)?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
        let reader = builder.build()?;

        for batch_result in reader {
            let batch = batch_result?;
            let num_rows = batch.num_rows();

            let id_col = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .cloned();

            let lat_col = batch
                .column_by_name("lat")
                .and_then(|c| c.as_any().downcast_ref::<Float64Array>())
                .cloned();

            let lon_col = batch
                .column_by_name("lon")
                .and_then(|c| c.as_any().downcast_ref::<Float64Array>())
                .cloned();

            let category_col = batch
                .column_by_name("category")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .cloned();

            let names_col = batch
                .column_by_name("names")
                .and_then(|c| c.as_any().downcast_ref::<StructArray>())
                .cloned();

            let primary_name_col = names_col.as_ref().and_then(|s| {
                let idx = s.column_names().iter().position(|n| *n == "primary")?;
                s.column(idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .cloned()
            });

            let addr_col = batch
                .column_by_name("addresses")
                .and_then(|c| c.as_any().downcast_ref::<StructArray>())
                .cloned();

            let addr_street_col = addr_col.as_ref().and_then(|s| {
                let idx = s.column_names().iter().position(|n| *n == "street")?;
                s.column(idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .cloned()
            });

            let addr_city_col = addr_col.as_ref().and_then(|s| {
                let idx = s.column_names().iter().position(|n| *n == "locality")?;
                s.column(idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .cloned()
            });

            let addr_region_col = addr_col.as_ref().and_then(|s| {
                let idx = s.column_names().iter().position(|n| *n == "region")?;
                s.column(idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .cloned()
            });

            let addr_postcode_col = addr_col.as_ref().and_then(|s| {
                let idx = s.column_names().iter().position(|n| *n == "postcode")?;
                s.column(idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .cloned()
            });

            let addr_country_col = addr_col.as_ref().and_then(|s| {
                let idx = s.column_names().iter().position(|n| *n == "country")?;
                s.column(idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .cloned()
            });

            for i in 0..num_rows {
                let (lat, lon) = match (&lat_col, &lon_col) {
                    (Some(lats), Some(lons)) if !lats.is_null(i) && !lons.is_null(i) => {
                        (lats.value(i), lons.value(i))
                    }
                    _ => continue,
                };

                if !AFRICA.contains(lon, lat) {
                    continue;
                }

                let id_str = id_col
                    .as_ref()
                    .filter(|c| !c.is_null(i))
                    .map(|c| c.value(i).to_string())
                    .unwrap_or_else(|| format!("overture-{i}"));

                let category_str = category_col
                    .as_ref()
                    .filter(|c| !c.is_null(i))
                    .map(|c| c.value(i));

                let category = match category_str.and_then(Category::from_str_opt) {
                    Some(cat) => cat,
                    None => continue,
                };

                let primary_name = primary_name_col
                    .as_ref()
                    .filter(|c| !c.is_null(i))
                    .map(|c| c.value(i).to_string());

                let names = match primary_name {
                    Some(name) if !name.is_empty() => vec![(Lang::En, name)],
                    _ => continue,
                };

                let address = build_address(
                    &addr_street_col,
                    &addr_city_col,
                    &addr_region_col,
                    &addr_postcode_col,
                    &addr_country_col,
                    i,
                );

                places.push(Place {
                    id: PlaceId::Overture(id_str),
                    names,
                    category,
                    lat,
                    lon,
                    address,
                    source: Source::Overture,
                });
            }
        }
    }

    Ok(places)
}

fn build_address(
    street: &Option<StringArray>,
    city: &Option<StringArray>,
    region: &Option<StringArray>,
    postcode: &Option<StringArray>,
    country: &Option<StringArray>,
    idx: usize,
) -> Option<Address> {
    let country_str = country
        .as_ref()
        .filter(|c| !c.is_null(idx))
        .map(|c| c.value(idx).to_string())
        .unwrap_or_default();

    let street_str = street
        .as_ref()
        .filter(|c| !c.is_null(idx))
        .map(|c| c.value(idx).to_string());
    let city_str = city
        .as_ref()
        .filter(|c| !c.is_null(idx))
        .map(|c| c.value(idx).to_string());
    let region_str = region
        .as_ref()
        .filter(|c| !c.is_null(idx))
        .map(|c| c.value(idx).to_string());
    let postcode_str = postcode
        .as_ref()
        .filter(|c| !c.is_null(idx))
        .map(|c| c.value(idx).to_string());

    if street_str.is_none() && city_str.is_none() && country_str.is_empty() {
        return None;
    }

    Some(Address {
        street: street_str,
        city: city_str,
        region: region_str,
        postcode: postcode_str,
        country: country_str,
    })
}

fn _check_geometry_lat_lon(
    batch: &arrow::record_batch::RecordBatch,
    i: usize,
) -> Option<(f64, f64)> {
    let geom_col = batch.column_by_name("geometry")?;
    if *geom_col.data_type() != DataType::Utf8 {
        return None;
    }
    let geom_arr = geom_col.as_any().downcast_ref::<StringArray>()?;
    if geom_arr.is_null(i) {
        return None;
    }
    let wkt = geom_arr.value(i);
    let stripped = wkt.trim_start_matches("POINT (").trim_end_matches(')');
    let mut parts = stripped.split_whitespace();
    let lon: f64 = parts.next()?.parse().ok()?;
    let lat: f64 = parts.next()?.parse().ok()?;
    Some((lat, lon))
}
