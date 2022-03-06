use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use encoding::{Encoding, DecoderTrap};
use encoding::all::{UTF_16LE, UTF_8};


/// To save a little bit of room, we dont care to index these fields
const EXLCUDE_LIST: &'static [&'static str] = &[
    "LastAccessTimeUtc",
    "LastWriteTimeUtc",
    "Sddl"
];


lazy_static! {
    /// Regexs for parsing values in the SystemInfo_ files
    static ref RE_OS_NAME: Regex = Regex::new(r"(?m)^OS Name:\s*([^\s].+?)\r?$").unwrap();
    static ref RE_OS_VERSION: Regex = Regex::new(r"(?m)^OS Version:\s*([^\s].+?)\r?$").unwrap();
}


/// Search a folder for all the SystemInfo_* files.
/// We assume that each folder that conains this file has an associated
/// file list.
fn get_system_info_files(path: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }
        if entry_path.file_name().unwrap().to_string_lossy().starts_with("") {
            paths.push(entry_path.to_path_buf());
        }
    }
    paths
}


/// Search a folder for csv files and return the found paths as PathBufs.
fn get_csv_files(path: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }
        if entry_path.file_name().unwrap().to_string_lossy().ends_with(".csv") {
            paths.push(entry_path.to_path_buf());
        }
    }
    paths
}


/// WindowsFileList represents a SystemInfo_ file and a CSV file list that should
/// be paired together.
#[derive(Debug)]
pub struct WindowsFileList {
    system_info_path: PathBuf,
    file_list_path: PathBuf
}
impl WindowsFileList {
    /// Create a WindowsFileList from a folder. Errors out if SystemInfo_ and CSV
    /// file can't be found.
    pub fn from_folder(path: impl AsRef<Path>) -> Result<Self, String> {
        // Find SystemInfo_ file in folder
        let mut sys_file_path_vec = get_system_info_files(&path);
        // Find CSV file in folder
        let mut csv_file_path_vec = get_csv_files(&path);

        let path = path.as_ref();

        // We should ONLY have 1 of each file.
        if sys_file_path_vec.len() != 1 {
            return Err(format!(
                "{} SystemInfo files were found in path {}",
                sys_file_path_vec.len(),
                path.display()
            ))
        }
        if csv_file_path_vec.len() != 1 {
            return Err(format!(
                "{} csv files were found in path {}",
                sys_file_path_vec.len(),
                path.display()
            ))
        }

        let system_info_path = sys_file_path_vec.pop()
            .expect("No SystemInfo_* file found!");
        let file_list_path = csv_file_path_vec.pop()
            .expect("No CSV file found!");

        Ok( WindowsFileList{
            system_info_path,
            file_list_path
        })
    }

    /// Get a WinFileListRecordIterator based off of the SystemInfo_/CSV
    /// pair. The iterator adds fields from the SystemInfo_ to the CSV values.
    pub fn into_iter(&self) -> Result<WinFileListRecordIterator, String> {
        // Get the Windows info from the SystemInfo_ file
        let win_info = json!(WindowsInfo::from_path(&self.system_info_path)?);
        // Create CSV reader for the file list
        let mut csv_rdr = csv::ReaderBuilder::new()
            .delimiter(b',')
            .from_path(&self.file_list_path.as_path())
            .map_err(|e|format!("{:?}", e))?;
        // Generate a header list
        let header: Vec<String> = csv_rdr.headers()
            .map_err(|e| format!("{:?}", e))?
            .iter()
            .map(|v|v.to_string())
            .collect();

        Ok( WinFileListRecordIterator {
            win_info,
            header,
            reader: csv_rdr
        })
    }
}


/// This iterator reads from CSV file and returns records with the Windows Info included
pub struct WinFileListRecordIterator {
    pub win_info: Value,
    pub header: Vec<String>,
    reader: csv::Reader<File>
}
impl WinFileListRecordIterator {
    /// Helper function to get headers from not just the CSV file, but also the Windows Info
    pub fn get_headers(&self) -> Vec<String>{
        let value = self.win_info.clone();
        let mut columns: Vec<String> = value.as_object()
            .expect("win_info is not an object!")
            .keys()
            .map(|v|v.to_string())
            .collect();
        let c2 = self.header.iter()
            .map(|v|v.clone())
            .collect::<Vec<String>>();

        columns.extend(c2);
        columns
    }
}
impl Iterator for WinFileListRecordIterator {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(result) = self.reader.records().next() {
                let record = match result {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("{:?}", e);
                        continue;
                    }
                };

                let mut value = self.win_info.clone();
                for (i, column) in self.header.iter().enumerate() {
                    if EXLCUDE_LIST.contains(&column.as_str()) {
                        continue;
                    }
                    value[column] = json!(&record[i]);
                }
                
                return Some(value);
            } else {
                break
            }
        }

        None
    }
}


/// Structure that represents the SystemInfo_ file. To add more fields to the output
/// add them to this struct and parse accordingly.
#[derive(Debug, Serialize)]
pub struct WindowsInfo {
    #[serde(rename = "OsName")]
    pub name: String,
    #[serde(rename = "OsVersion")]
    pub version: String,
}
impl WindowsInfo {
    /// Create this structure from a SystemInfo_ file path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();

        // File pre checks
        if !path.is_file() {
            return Err(format!(
                "{} is not a file!", path.to_string_lossy()
            ));
        }
        let meta = path.metadata().map_err(|e|format!("{:?}", e))?;
        if meta.len() > 1024 * 1024 * 4 {
            return Err(format!(
                "{} is to large!", path.to_string_lossy()
            ));
        }

        // Open the file from a path
        let mut fh = File::open(&path)
            .map_err(|e|format!(
                "Could not open '{}'. {:?}",
                path.to_string_lossy(),
                e
            )
        )?;

        // Read file content to string
        let mut buffer = Vec::new();
        fh.read_to_end(&mut buffer).map_err(|e|format!(
            "Error reading into buffer! {:?}", e
        ))?;

        // Some powershell output is in utf16, some is in utf8
        let content = if &buffer[0..2] == &[0xff, 0xfe] {
            match UTF_16LE.decode(&buffer, DecoderTrap::Ignore) {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return Err(format!("Error decoding utf16le: {:?}", e));
                }
            }
        } else {
            match UTF_8.decode(&buffer, DecoderTrap::Ignore) {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return Err(format!("Error decoding utf8: {:?}", e));
                }
            }
        };

        // Search for needed variables
        let name = RE_OS_NAME.captures(&content)
            .ok_or(format!(
                "Unable to parse OS Name for '{}'", path.to_string_lossy()
            ))?
            .get(1)
            .expect("Name group not valid.")
            .as_str()
            .to_owned();
        let version = RE_OS_VERSION.captures(&content)
            .ok_or(format!(
                "Unable to parse OS Version for '{}'", path.to_string_lossy()
            ))?
            .get(1)
            .expect("Version group not valid.")
            .as_str()
            .to_owned();

        Ok(
            WindowsInfo {
                name,
                version
            }
        )
    }
}


/// This Iterator returns a tuple (PathBuf, WindowsFileList) for each sub folder
/// in a recursive folder that contains file lists. This makes it convenient for
/// iterating the VanillaWindowsReference repo.
pub struct WinFileListIterator{
    dir: walkdir::IntoIter
}
impl WinFileListIterator{
    pub fn from_path(path: impl AsRef<Path>) -> Self {
        let dir = WalkDir::new(&path.as_ref())
            .into_iter();

        Self { dir }
    }
}
impl Iterator for WinFileListIterator {
    type Item = (PathBuf, WindowsFileList);
    
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(entry) = &self.dir.next() {
            // Get the dir entry result
            let entry = match entry {
                Err(e) => {
                    error!("Error reading dir entry: {:?}", e);
                    continue;
                },
                Ok(e) => e
            };
            // Get the path
            let entry_path = entry.path();
            // Make sure its a file
            if !entry_path.is_file() {
                continue;
            }
            // We are looking for the SystemInfo file
            if entry_path.file_name().unwrap().to_string_lossy().starts_with("SystemInfo_") {
                let parent = entry.path()
                    .parent()
                    .expect("Could not get entries' parent.");

                match WindowsFileList::from_folder(&parent) {
                    Ok(fl) => {
                        return Some((parent.to_path_buf(), fl));
                    },
                    Err(e) => {
                        error!("{:?}", e);
                        continue;
                    }
                }
            }
        }

        None
    }
}
