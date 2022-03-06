use std::collections::{HashMap, HashSet};
use std::path::Path;
use serde::{Serialize, Deserialize};
use serde_json::json;
use lazy_static::lazy_static;
use regex::Regex;
use rocket::{post, State};
use rocket::serde::json::Json;
use crate::error::VanillaError;
use crate::index::WindowsRefIndexReader;

lazy_static! {
    static ref RE_LETTER: Regex = Regex::new(r"(?i)^[a-z]:\\").unwrap();
}


#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct FileNameLookup {
    value: String,
    path: Option<String>
}


#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct FullPathLookup {
    value: String
}
impl FullPathLookup {
    fn as_file_name_lookup(&self) -> Result<FileNameLookup, VanillaError> {
        let full_path = Path::new(&self.value);

        let value = full_path.file_name()
            .ok_or(VanillaError::from_message(
                format!("Could not get file_name from {}", full_path.to_string_lossy())
            ))?
            .to_string_lossy()
            .to_string();

        let path = full_path.parent()
            .ok_or(VanillaError::from_message(
                format!("Could not get parent from {}", full_path.to_string_lossy())
            ))?
            .to_string_lossy()
            .to_string();

        Ok( FileNameLookup { value, path: Some(path) })
    }
}


fn resolve(
    key: &str,
    lookup: &FileNameLookup,
    index_reader: &State<WindowsRefIndexReader>,
) -> serde_json::Value {
    let mut aggregation: HashMap<String, HashSet<String>> = HashMap::new();
    let known_value;
    let mut known_path = None;

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
    if let Some(path) = &lookup.path {
        if let Some(dir_names) = aggregation.get("DirectoryName") {
            let lower_paths = dir_names.iter()
                .map(|v|v.to_lowercase())
                .collect::<Vec<String>>();
            
            let search_v = &path.to_lowercase()
                .replace(r"/", r"\");

            if lower_paths.contains(search_v) {
                known_path = Some(true);
            } else {
                known_path = Some(false);
            }
        }
    }

    return_value["KnownName"] = json!(known_value);
    return_value["KnownPath"] = json!(known_path);

    return_value
}



#[post("/api/v1/lookup/name", format="json", data="<name_lookup>")]
pub fn lookup_file_name(
    index_reader: &State<WindowsRefIndexReader>,
    mut name_lookup: Json<FileNameLookup>
) -> serde_json::Value {
    if let Some(path) = name_lookup.path.as_mut() {
        *path = path.replace(r"/", r"\");
        *path = path.trim_start_matches(r"\").to_string();
        *path = RE_LETTER.replace(&path, "").to_string();
    }
    resolve("Name", &name_lookup, index_reader)
}


#[post("/api/v1/lookup/name", format="json", data="<name_lookup>")]
pub fn lookup_full_name(
    index_reader: &State<WindowsRefIndexReader>,
    mut name_lookup: Json<FullPathLookup>
) -> serde_json::Value {
    name_lookup.value = name_lookup.value.replace(r"/", r"\");
    name_lookup.value = name_lookup.value.trim_start_matches(r"\").to_string();
    name_lookup.value = RE_LETTER.replace(&name_lookup.value, "").to_string();

    let name_lookup = name_lookup.into_inner()
        .as_file_name_lookup()
        .expect("Error converting FullPathLookup to FileNameLookup");

    resolve("Name", &name_lookup, index_reader)
}
