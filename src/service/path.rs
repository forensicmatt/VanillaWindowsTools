use std::time::{Duration, Instant};
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
impl FileNameLookup {
    fn get_lookup_path(&self) -> Option<String> {
        if let Some(path) = &self.path {
            Some(path.replace(r"/", r"\").to_lowercase())
        } else {
            None
        }
    }
}


#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct FullPathLookup {
    value: String
}
impl FullPathLookup {
    fn as_file_name_lookup(&self) -> Result<FileNameLookup, VanillaError> {
        let os_path = self.as_os_path();
        let full_path = Path::new(&os_path);

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

    fn as_os_path(&self) -> String {
        #[cfg(target_family="windows")]
        {self.value.replace("/", r"\")}
        #[cfg(not(target_family="windows"))]
        {self.value.replace(r"\", "/")}
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
    if let Some(path) = &lookup.get_lookup_path() {
        if let Some(dir_names) = aggregation.get("DirectoryName") {
            let lower_paths = dir_names.iter()
                .map(|v|v.to_lowercase())
                .collect::<Vec<String>>();

            if lower_paths.contains(path) {
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


pub fn known_lookup(
    name_lookup: &FileNameLookup,
    index_reader: &State<WindowsRefIndexReader>,
) -> serde_json::Value {
    let mut result = json!({});

    // Create index query
    let query_known_name = format!("Name:\"{}\"", &name_lookup.value.to_lowercase());

    // Get hits
    let hits = index_reader.get_query_hits(&query_known_name, 1)
        .expect("Query error.");

    if hits.len() > 0 {
        result["KnownName"] = json!(true);
    } else {
        result["KnownName"] = json!(false);
    }

    if let Some(path) = &name_lookup.get_lookup_path() {
        let query_known_path = format!(
            "Name:\"{}\" AND DirectoryName:\"{}\"", 
            &name_lookup.value.to_lowercase(),
            path
        );
        // Get hits
        let hits = index_reader.get_query_hits(&query_known_path, 1)
            .expect("Query error.");
        
        if hits.len() > 0 {
            result["KnownPath"] = json!(true);
        } else {
            result["KnownPath"] = json!(false);
        }
    } else {
        result["KnownPath"] = serde_json::Value::Null;
    }

    result
}


#[post("/api/v1/known/name", format="json", data="<name_lookup>")]
pub fn known_file_name(
    index_reader: &State<WindowsRefIndexReader>,
    mut name_lookup: Json<FileNameLookup>
) -> serde_json::Value {
    let start = Instant::now();

    if let Some(path) = name_lookup.path.as_mut() {
        *path = path.trim_start_matches(r"\").to_string();
        *path = path.trim_start_matches(r"/").to_string();
        *path = RE_LETTER.replace(&path, "").to_string();
    }

    let result = known_lookup(
        &name_lookup,
        index_reader
    );

    let duration = start.elapsed();
    info!("Time elapsed in known_file_name() is: {:?}", duration);

    result
}


#[post("/api/v1/known/fullname", format="json", data="<name_lookup>")]
pub fn known_full_name(
    index_reader: &State<WindowsRefIndexReader>,
    mut name_lookup: Json<FullPathLookup>
) -> serde_json::Value {
    let start = Instant::now();

    name_lookup.value = name_lookup.value.trim_start_matches(r"\").to_string();
    name_lookup.value = name_lookup.value.trim_start_matches(r"/").to_string();
    name_lookup.value = RE_LETTER.replace(&name_lookup.value, "").to_string();

    let name_lookup = name_lookup.into_inner()
        .as_file_name_lookup()
        .expect("Error converting FullPathLookup to FileNameLookup");

    let result = known_lookup(&name_lookup, index_reader);

    let duration = start.elapsed();
    info!("Time elapsed in known_full_name() is: {:?}", duration);

    result
}


#[post("/api/v1/lookup/name", format="json", data="<name_lookup>")]
pub fn lookup_file_name(
    index_reader: &State<WindowsRefIndexReader>,
    mut name_lookup: Json<FileNameLookup>
) -> serde_json::Value {
    let start = Instant::now();

    if let Some(path) = name_lookup.path.as_mut() {
        *path = path.trim_start_matches(r"\").to_string();
        *path = path.trim_start_matches(r"/").to_string();
        *path = RE_LETTER.replace(&path, "").to_string();
    }
    let result = resolve("Name", &name_lookup, index_reader);

    let duration = start.elapsed();
    info!("Time elapsed in lookup_file_name() is: {:?}", duration);

    result
}


#[post("/api/v1/lookup/fullname", format="json", data="<name_lookup>")]
pub fn lookup_full_name(
    index_reader: &State<WindowsRefIndexReader>,
    mut name_lookup: Json<FullPathLookup>
) -> serde_json::Value {
    let start = Instant::now();

    name_lookup.value = name_lookup.value.trim_start_matches(r"/").to_string();
    name_lookup.value = name_lookup.value.trim_start_matches(r"\").to_string();
    name_lookup.value = RE_LETTER.replace(&name_lookup.value, "").to_string();

    let name_lookup = name_lookup.into_inner()
        .as_file_name_lookup()
        .expect("Error converting FullPathLookup to FileNameLookup");

    let result = resolve(
        "Name",
        &name_lookup,
        index_reader
    );

    let duration = start.elapsed();
    info!("Time elapsed in lookup_full_name() is: {:?}", duration);

    result
}
