use std::path::{Path, PathBuf};
use std::env::temp_dir;
use std::collections::HashSet;
use tantivy::schema::*;
use tantivy::{Index, Document, IndexWriter, IndexReader};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use serde_json::{json, Value};
use crate::error::VanillaError;
use crate::tokenizer::RawLowerTokenizer;
use crate::vanilla::WinFileListIterator;

const FIELDS_STRING: &[&'static str] = &["DirectoryName", "Name", "MD5", "SHA256"];
const FIELDS_EXCLUDE: &[&'static str] = &["Attributes", "Sddl"];


/// Get the list of fields that should be added to the index.
fn get_index_fields(path: impl AsRef<Path>) -> HashSet<String> {
    let mut columns = HashSet::new();
    let iter = WinFileListIterator::from_path(path);
    for (location, file_list) in iter {
        let record_iter = match file_list.into_iter(){
            Ok(i) => i,
            Err(e) => {
                error!(
                    "Error handling file list in {}: {}", 
                    location.to_string_lossy(), e
                );
                continue;
            }
        };
        columns.extend(record_iter.get_headers());
    }
    columns
}

fn get_schema(path: impl AsRef<Path>) -> Schema {
    let fields = get_index_fields(path);
    let mut schema_builder = Schema::builder();

    for field in &fields {
        if FIELDS_EXCLUDE.contains(&field.as_str()) {
            continue
        }

        let text_field_indexing = TextFieldIndexing::default()
            .set_tokenizer("rawlower");

        let text_options = TextOptions::default()
            .set_indexing_options(text_field_indexing)
            .set_stored();
        
        let _field = if FIELDS_STRING.contains(&field.as_str()) {
            schema_builder.add_text_field(
                &field, 
                text_options | STORED
            )
        } else {
            schema_builder.add_text_field(
                &field, 
                STORED
            )
        };
    }

    schema_builder.build()
}


#[derive(Clone)]
pub struct WindowsRefIndexOptions {
    pub cleanup: bool
}
impl Default for WindowsRefIndexOptions {
    fn default() -> Self {
        Self {
            cleanup: false
        }
    }
}


#[derive(Clone)]
pub struct WindowRefIndex {
    /// The path to the Vanilla Reference files
    windows_ref_path: PathBuf,
    /// The index path
    index_path: PathBuf,
    /// Can be used in the future for indexing options
    options: WindowsRefIndexOptions,
    index: Index,
    /// A flag that gets set if the index was pre existing or not
    pub pre_existing: bool
}
impl WindowRefIndex {
    pub fn new(
        windows_ref_path: impl AsRef<Path>,
        index_path: impl AsRef<Path>,
        options: WindowsRefIndexOptions
    ) -> Result<Self, VanillaError> {
        let windows_ref_path = windows_ref_path.as_ref();
        let index_path = index_path.as_ref();

        // This is the file that will exist if the Index already exists
        let mut meta_path = index_path.to_path_buf().clone();
        meta_path.push("meta.json");

        // If the index foder does not exist, we need to create it
        if !index_path.is_dir() {
            std::fs::create_dir_all(index_path)?;
        }

        // Create the Index from a pre existing path or create it
        let (index, pre_existing) = if meta_path.is_file() {
            (
                Index::open_in_dir(index_path)?,
                true
            )
        } else {
            let schema = get_schema(windows_ref_path);
            (
                Index::create_in_dir(index_path, schema)?,
                false
            )
        };
        
        // Register our custom Tokenizer
        index
            .tokenizers()
            .register("rawlower", RawLowerTokenizer);

        Ok( Self {
            windows_ref_path: windows_ref_path.to_path_buf().clone(),
            index_path: index_path.to_owned(),
            index,
            options,
            pre_existing
        })
    }

    pub fn from_paths(
        windows_ref_path: impl AsRef<Path>,
        index_path: Option<impl AsRef<Path>>,
        options: Option<WindowsRefIndexOptions>
    ) -> Result<Self, VanillaError> {
        let windows_ref_path = windows_ref_path.as_ref();

        // If no index path was passed, use a temparary location
        let index_path = if let Some(p) = index_path {
            p.as_ref().to_path_buf().clone()
        } else {
            let mut temp_path = temp_dir();
            let location_name = windows_ref_path.file_name()
                .ok_or_else(||VanillaError::from_message("Unable to get path name.".to_string()))?;

            temp_path.push(location_name);
            temp_path
        };

        // Use passed options or default
        let options = options.unwrap_or(WindowsRefIndexOptions::default());

        WindowRefIndex::new(
            windows_ref_path.to_owned(),
            index_path,
            options
        )
    }

    pub fn get_writer<'a>(&'a self) -> Result<WindowRefIndexWriter<'a>, VanillaError> {
        WindowRefIndexWriter::new(self)
    }

    pub fn get_reader(&self) -> Result<WindowsRefIndexReader, VanillaError> {
        let index = self.clone();
        WindowsRefIndexReader::new(index)
    }
}

pub struct WindowsRefIndexReader {
    win_ref_index: WindowRefIndex,
    index_reader: IndexReader
}
impl WindowsRefIndexReader {
    pub fn new(
        win_ref_index: WindowRefIndex
    ) -> Result<Self, VanillaError> {
        let index_reader = win_ref_index.index
            .reader()?;

        Ok( Self {
            win_ref_index,
            index_reader
        })
    }   

    pub fn get_query_parser(
        &self,
        query: &str
    ) -> Result<Box<(dyn tantivy::query::Query + 'static)>, VanillaError> {
        let schema = self.win_ref_index.index.schema();
        let schema_vec = schema.fields().map(|v|v.0).collect();
        let query_parser = QueryParser::for_index(
            &self.win_ref_index.index, 
            schema_vec
        );
        query_parser.parse_query(query)
            .map_err(|e|VanillaError::from_message(format!("{:?}", e)))

    }

    pub fn get_query_hits(
        &self, 
        query: &str,
        limit: usize
    ) -> Result<Vec<Value>, VanillaError>{
        let q = self.get_query_parser(query)?;
        let s = self.index_reader.searcher();
        let docs = s.search(
            &q,
            &TopDocs::with_limit(limit)
        )?;

        let mut records = Vec::new();
        let schema = self.win_ref_index.index.schema();
        for (_score, doc_address) in docs {
            // Retrieve the actual content of documents given its `doc_address`.
            let retrieved_doc = s.doc(doc_address)?;
            let named_doc = schema.to_named_doc(&retrieved_doc);
            records.push(json!(&named_doc));
        }
        Ok(records)
    }
}


pub struct WindowRefIndexWriter<'a> {
    win_ref_index: &'a WindowRefIndex,
    index_writer: IndexWriter
}
impl<'a> WindowRefIndexWriter<'a> {
    pub fn new(
        win_ref_index: &'a WindowRefIndex
    ) -> Result<Self, VanillaError> {
        let index_writer = win_ref_index.index
            .writer(100_000_000)?;

        Ok( Self {
            win_ref_index,
            index_writer
        })
    }
    
    /// Perform index operations
    pub fn index(&mut self) -> Result<(), VanillaError> {
        info!(
            "[starting] Indexing path: {}",
            &self.win_ref_index.windows_ref_path.to_string_lossy()
        );

        let schema = self.win_ref_index.index.schema();

        let file_list_iter = WinFileListIterator::from_path(
            &self.win_ref_index.windows_ref_path
        );
        for (location, file_list) in file_list_iter {
            // Get the record iterator from the file list
            let record_iter = match file_list.into_iter(){
                Ok(i) => i,
                Err(e) => {
                    error!(
                        "Error handling file list in {}: {}", 
                        location.to_string_lossy(), e
                    );
                    continue;
                }
            };

            info!("[starting] Indexing path: {}", location.to_string_lossy());
            // Iterate each record
            for mut record in record_iter {
                let mut doc = Document::new();
                // index conversions (all in lowercase)
                if let Some(n) = record["DirectoryName"].as_str() {
                    record["DirectoryName"] = json!(n[3..]);
                }
                if let Some(n) = record["FullName"].as_str() {
                    record["FullName"] = json!(n[3..]);
                }
                if let Some(n) = record["MD5"].as_str() {
                    record["MD5"] = json!(n);
                }
                if let Some(n) = record["SHA256"].as_str() {
                    record["SHA256"] = json!(n);
                }

                for (field, field_entry) in schema.fields() {
                    let field_name = field_entry.name();

                    // Skip indexing excluded fields
                    if FIELDS_EXCLUDE.contains(&field_name) {
                        continue
                    }
                    if let Some(field_value) = record.get(field_name) {
                        if let Some(fv_str) = field_value.as_str() {
                            doc.add_text(
                                field,
                                fv_str
                            );
                        }
                    }
                }

                self.index_writer.add_document(doc);
            }
            self.index_writer.commit()?;

            info!("[finished] Indexing path: {}", location.to_string_lossy());
        }

        info!(
            "[finished] Indexing path: {}",
            &self.win_ref_index.windows_ref_path.to_string_lossy()
        );

        Ok(())
    }
}
