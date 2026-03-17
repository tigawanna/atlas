use atlas_core::{LandmarkRef, Lang, Locality, ParsedQuery, SpatialRelation};

use crate::tokenizer::detect_lang;

struct SpatialKeyword {
    keyword: &'static str,
    relation: SpatialRelation,
}

fn english_keywords() -> Vec<SpatialKeyword> {
    vec![
        SpatialKeyword {
            keyword: "near",
            relation: SpatialRelation::Near,
        },
        SpatialKeyword {
            keyword: "behind",
            relation: SpatialRelation::Behind,
        },
        SpatialKeyword {
            keyword: "past",
            relation: SpatialRelation::Past,
        },
        SpatialKeyword {
            keyword: "beside",
            relation: SpatialRelation::Beside,
        },
        SpatialKeyword {
            keyword: "opposite",
            relation: SpatialRelation::Opposite,
        },
    ]
}

fn french_keywords() -> Vec<SpatialKeyword> {
    vec![
        SpatialKeyword {
            keyword: "près de",
            relation: SpatialRelation::Near,
        },
        SpatialKeyword {
            keyword: "derrière",
            relation: SpatialRelation::Behind,
        },
        SpatialKeyword {
            keyword: "après",
            relation: SpatialRelation::Past,
        },
        SpatialKeyword {
            keyword: "à côté de",
            relation: SpatialRelation::Beside,
        },
        SpatialKeyword {
            keyword: "entre",
            relation: SpatialRelation::Between(String::new()),
        },
        SpatialKeyword {
            keyword: "en face de",
            relation: SpatialRelation::Opposite,
        },
    ]
}

fn arabic_keywords() -> Vec<SpatialKeyword> {
    vec![
        SpatialKeyword {
            keyword: "بالقرب من",
            relation: SpatialRelation::Near,
        },
        SpatialKeyword {
            keyword: "خلف",
            relation: SpatialRelation::Behind,
        },
        SpatialKeyword {
            keyword: "بعد",
            relation: SpatialRelation::Past,
        },
        SpatialKeyword {
            keyword: "بجانب",
            relation: SpatialRelation::Beside,
        },
        SpatialKeyword {
            keyword: "بين",
            relation: SpatialRelation::Between(String::new()),
        },
        SpatialKeyword {
            keyword: "مقابل",
            relation: SpatialRelation::Opposite,
        },
    ]
}

fn keywords_for_lang(lang: &Option<Lang>) -> Vec<SpatialKeyword> {
    match lang {
        Some(Lang::Fr) => french_keywords(),
        Some(Lang::Ar) => arabic_keywords(),
        _ => english_keywords(),
    }
}

const STREET_SUFFIXES: &[&str] = &["Street", "Road", "Avenue", "Blvd", "Lane"];

fn ends_with_street_suffix(text: &str) -> bool {
    let trimmed = text.trim();
    STREET_SUFFIXES
        .iter()
        .any(|suffix| trimmed.ends_with(suffix))
}

fn extract_street_name(text: &str) -> Option<String> {
    if ends_with_street_suffix(text) {
        Some(text.trim().to_string())
    } else {
        None
    }
}

fn extract_locality_from_segments<'a>(segments: &'a [&'a str]) -> (Option<Locality>, Vec<&'a str>) {
    let n = segments.len();
    if n == 0 {
        return (None, segments.to_vec());
    }

    let locality_count = if n >= 3 { 2 } else { 1 };
    let locality_segments = &segments[n - locality_count..];
    let remaining = &segments[..n - locality_count];

    let locality = match locality_count {
        1 => Locality {
            neighborhood: None,
            city: Some(locality_segments[0].trim().to_string()),
            country: None,
        },
        2 => Locality {
            neighborhood: Some(locality_segments[0].trim().to_string()),
            city: Some(locality_segments[1].trim().to_string()),
            country: None,
        },
        _ => return (None, segments.to_vec()),
    };

    (Some(locality), remaining.to_vec())
}

fn find_spatial_relation(
    text: &str,
    keywords: &[SpatialKeyword],
) -> Option<(SpatialRelation, String, String)> {
    let lower = text.to_lowercase();
    for kw in keywords {
        let kw_lower = kw.keyword.to_lowercase();
        if let Some(pos) = lower.find(&kw_lower) {
            let after = &text[pos + kw.keyword.len()..].trim_start().to_string();
            let before = text[..pos].trim_end().to_string();
            let relation = match &kw.relation {
                SpatialRelation::Near => SpatialRelation::Near,
                SpatialRelation::Behind => SpatialRelation::Behind,
                SpatialRelation::Past => SpatialRelation::Past,
                SpatialRelation::Beside => SpatialRelation::Beside,
                SpatialRelation::Between(_) => SpatialRelation::Between(String::new()),
                SpatialRelation::Opposite => SpatialRelation::Opposite,
            };
            return Some((relation, before, after.to_string()));
        }
    }
    None
}

pub fn parse(text: &str, lang_hint: Option<&Lang>) -> ParsedQuery {
    let lang = detect_lang(text, lang_hint);
    let keywords = keywords_for_lang(&lang);

    let comma_segments: Vec<&str> = text.split(',').collect();

    let (locality, query_segments) = if comma_segments.len() > 1 {
        extract_locality_from_segments(&comma_segments)
    } else {
        (None, comma_segments.clone())
    };

    let query_text_owned: String = query_segments.join(",");
    let query_text: &str = query_text_owned.trim();

    if let Some((relation, _before, landmark_text)) = find_spatial_relation(query_text, &keywords) {
        let landmark_name = landmark_text.trim().to_string();
        let landmark_ref = if landmark_name.is_empty() {
            None
        } else {
            Some(LandmarkRef {
                name: landmark_name,
                relation,
            })
        };

        return ParsedQuery {
            tokens: vec![],
            lang,
            locality,
            landmark_ref,
            street: None,
            descriptors: vec![],
        };
    }

    if let Some(street) = extract_street_name(query_text) {
        return ParsedQuery {
            tokens: vec![],
            lang,
            locality,
            landmark_ref: None,
            street: Some(street),
            descriptors: vec![],
        };
    }

    let tokens: Vec<String> = query_text
        .split_whitespace()
        .filter(|s: &&str| !s.is_empty())
        .map(|s: &str| s.to_string())
        .collect();

    ParsedQuery {
        tokens,
        lang,
        locality,
        landmark_ref: None,
        street: None,
        descriptors: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_locality() {
        let result = parse("Makola Market, Accra", None);
        assert!(result.locality.is_some());
        let loc = result.locality.unwrap();
        assert_eq!(loc.city.as_deref(), Some("Accra"));
        assert_eq!(result.tokens, vec!["Makola", "Market"]);
    }

    #[test]
    fn parse_landmark_near_with_two_locality_segments() {
        let result = parse("near MTN mast, Osu, Accra", None);
        assert!(result.landmark_ref.is_some());
        let lref = result.landmark_ref.unwrap();
        assert_eq!(lref.name, "MTN mast");
        assert!(matches!(lref.relation, SpatialRelation::Near));
        let loc = result.locality.unwrap();
        assert_eq!(loc.neighborhood.as_deref(), Some("Osu"));
        assert_eq!(loc.city.as_deref(), Some("Accra"));
    }

    #[test]
    fn parse_street_with_locality() {
        let result = parse("Oxford Street, Osu", None);
        assert_eq!(result.street.as_deref(), Some("Oxford Street"));
        let loc = result.locality.unwrap();
        assert_eq!(loc.city.as_deref(), Some("Osu"));
    }

    #[test]
    fn parse_plain_tokens() {
        let result = parse("Accra Mall", None);
        assert_eq!(result.tokens, vec!["Accra", "Mall"]);
        assert!(result.locality.is_none());
        assert!(result.landmark_ref.is_none());
        assert!(result.street.is_none());
    }

    #[test]
    fn parse_no_crash_on_empty() {
        let result = parse("", None);
        assert!(result.tokens.is_empty());
        assert!(result.locality.is_none());
    }

    #[test]
    fn parse_single_segment_becomes_city() {
        let result = parse("Market, Lagos", None);
        let loc = result.locality.unwrap();
        assert_eq!(loc.city.as_deref(), Some("Lagos"));
    }
}
