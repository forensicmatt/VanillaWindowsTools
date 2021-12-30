use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Serialize};
use serde_json::{json, Value};
use encoding::{Encoding, DecoderTrap};
use encoding::all::{UTF_16LE, UTF_8};

const EXLCUDE_LIST: &'static [&'static str] = &[
    "LastAccessTimeUtc",
    "LastWriteTimeUtc",
    "Sddl",
    "DirectoryName"
];

lazy_static! {
    static ref RE_OS_NAME: Regex = Regex::new(r"(?m)^OS Name:\s*([^\s].+?)\r?$").unwrap();
    static ref RE_OS_VERSION: Regex = Regex::new(r"(?m)^OS Version:\s*([^\s].+?)\r?$").unwrap();
}


fn get_system_info_files(path: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }
        if entry_path.file_name().unwrap().to_string_lossy().starts_with("SystemInfo_") {
            paths.push(entry_path.to_path_buf());
        }
    }
    paths
}

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


#[derive(Debug)]
pub struct WindowsFileList {
    system_info_path: PathBuf,
    file_list_path: PathBuf
}
impl WindowsFileList {
    pub fn from_folder(path: impl AsRef<Path>) -> Result<Self, String> {
        let mut sys_file_path_vec = get_system_info_files(&path);
        let mut csv_file_path_vec = get_csv_files(&path);

        let path = path.as_ref();

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

        let system_info_path = sys_file_path_vec.pop().expect("No path to pop!");
        let file_list_path = csv_file_path_vec.pop().expect("No path to pop!");

        Ok( WindowsFileList{
            system_info_path,
            file_list_path
        })
    }

    pub fn into_iter(&self) -> Result<WindowsFileListIterator, String> {
        let win_info = json!(WindowsInfo::from_path(&self.system_info_path)?);
        let mut csv_rdr = csv::ReaderBuilder::new()
            .delimiter(b',')
            .from_path(&self.file_list_path.as_path())
            .map_err(|e|format!("{:?}", e))?;

        let header: Vec<String> = csv_rdr.headers()
            .map_err(|e| format!("{:?}", e))?
            .iter()
            .map(|v|v.to_string())
            .collect();

        Ok( WindowsFileListIterator {
            win_info,
            header,
            reader: csv_rdr
        })
    }
}


pub struct WindowsFileListIterator {
    win_info: Value,
    header: Vec<String>,
    reader: csv::Reader<File>
}
impl Iterator for WindowsFileListIterator {
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


#[derive(Debug, Serialize)]
pub struct WindowsFileEntry {
    pub system_info: WindowsInfo,
    pub file_info: Value
}
impl WindowsFileEntry {
}


#[derive(Debug, Serialize)]
pub struct WindowsInfo {
    #[serde(rename = "OsName")]
    pub name: String,
    #[serde(rename = "OsVersion")]
    pub version: String,
}
impl WindowsInfo {
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
