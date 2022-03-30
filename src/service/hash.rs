use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use serde_json::json;
use rocket::{post, State};
use rocket::serde::json::Json;
use crate::index::WindowsRefIndexReader;


#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct HashLookup {
    value: String
}


fn resolve(
    key: &str,
    lookup: &HashLookup,
    index_reader: &State<WindowsRefIndexReader>,
) -> serde_json::Value {
    let mut aggregation: HashMap<String, HashSet<String>> = HashMap::new();
    let known_value;

    // Create index query
    let query = format!("{}:\"{}\"", &key, &lookup.value.to_lowercase());

    // Get hits
    let mut hits = index_reader.get_query_hits(&query, 1000)
        .expect("Query error.");

    // Set aggregation from first record
    if let Some(first_record) = hits.pop() {
        let object = first_record.as_object()
            .expect("Record is not an object!");

        for (k, v) in object {
            let v = v[0].as_str()
                .expect(&format!("First value is not a string. {}", v));

            aggregation.insert(
                k.to_owned(),
                HashSet::from([v.to_owned()])
            );
        }
        known_value = Some(true);
    } else {
        known_value = Some(false);
    }

    // Aggregate the rest of the hits
    for hit in hits {
        if let Some(object) = hit.as_object() {
            for (k, v) in object {
                let v = v[0].as_str().expect("First value is not a string.");
                if let Some(set) = aggregation.get_mut(k) {
                    set.insert(v.to_owned());
                }
            }
        }
    }

    let mut return_value = json!(aggregation);

    return_value
}


#[post("/api/v1/lookup/hash", format="json", data="<hash_lookup>")]
pub fn lookup_hash(
    index_reader: &State<WindowsRefIndexReader>,
    hash_lookup: Json<HashLookup>
) -> Result<serde_json::Value, String> {
    let key = match hash_lookup.value.len() {
        32 => "MD5",
        64 => "SHA256",
        len => {
            return Err(format!("Unhandled hash type with length: {}", len));
        }
    };

    let result = resolve(key, &hash_lookup, index_reader);

    Ok( result )
}
