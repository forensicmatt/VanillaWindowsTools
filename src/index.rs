use std::path::{Path, PathBuf};
use std::collections::HashSet;
use git2::Repository;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tantivy::schema::*;
use tantivy::{Index, Document, IndexWriter, IndexReader};
use tantivy::{LeasedItem, Searcher};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::error::TantivyError;
use serde_json::{json, Value};
use crate::error::VanillaError;
use crate::tokenizer::RawLowerTokenizer;
use crate::vanilla::{WindowsFileList, WinFileListIterator, get_system_info_files};

const FIELDS_STRING: &[&'static str] = &["DirectoryName", "Name", "MD5", "SHA256"];
const FIELDS_EXCLUDE: &[&'static str] = &["Attributes", "Sddl"];

type SearchQuery = (LeasedItem<Searcher>, Box<(dyn tantivy::query::Query + 'static)>);


/// Clone the VanillaReference folder
pub fn clone_vanilla_reference_repo(
    destination: impl AsRef<Path>
) -> Result<(), VanillaError> {
    Repository::clone("https://github.com/AndrewRathbun/VanillaWindowsReference", destination)?;
    Ok(())
}


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


/// Create a schema based of the fields found in the Vanilla reference
/// file lists. This can be used for generating an Index.
pub fn generate_schema_from_vanilla(
    path: impl AsRef<Path>
) -> Result<Schema, String> {
    let path = path.as_ref();
    let fields = get_index_fields(&path);
    if fields.is_empty() {
        return Err(format!("Could not resolve any fields in {}", &path.to_string_lossy()));
    }

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

    Ok(schema_builder.build())
}


/// Index a file list
fn index_file_list(
    set_count: usize,
    tuple: &(usize, PathBuf, WindowsFileList),
    index_writer: &IndexWriter,
    schema: &Schema
) -> Result<(), VanillaError> {
    let (i, location, file_list) = tuple;

    let i = i + 1;
            
    // Get the record iterator from the file list
    let record_iter = file_list.into_iter()
        .map_err(|e| VanillaError::from_message(e))?;

    info!(
        "[starting {}/{}] Indexing path: {}",
        i, set_count,
        location.to_string_lossy()
    );
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

        index_writer.add_document(doc);
    }

    info!(
        "[finished {}/{}] Indexing path: {}",
        i, set_count,
        location.to_string_lossy()
    );

    Ok(())
}


/// Handle Index reading operations such as queries.
pub struct WindowsRefIndexReader {
    /// The IndexReader
    index_reader: IndexReader
}
impl WindowsRefIndexReader {
    /// Get the Searcher and Query struct for a given query string
    fn get_query(
        &self,
        query: &str
    ) -> Result<SearchQuery, VanillaError> {
        let searcher = self.index_reader.searcher();
        let schema = searcher.schema();
        let schema_vec = schema.fields()
            .map(|v|v.0)
            .collect();

        let index = searcher.index();

        let query_parser = QueryParser::for_index(
            index, 
            schema_vec
        );

        let query = query_parser.parse_query(query)
            .map_err(|e|VanillaError::from_message(format!("{:?}", e)))?;
        
        Ok((searcher, query))
    }

    /// Get a hits for a given query
    pub fn get_query_hits(
        &self, 
        query: &str,
        limit: usize
    ) -> Result<Vec<Value>, VanillaError>{
        let (searcher, query) = self.get_query(query)?;
        let docs = searcher.search(
            &query,
            &TopDocs::with_limit(limit)
        )?;
        let schema = searcher.schema();

        let mut records = Vec::new();
        for (_score, doc_address) in docs {
            // Retrieve the actual content of documents given its `doc_address`.
            let retrieved_doc = searcher.doc(doc_address)?;
            let named_doc = schema.to_named_doc(&retrieved_doc);
            records.push(json!(&named_doc));
        }

        Ok(records)
    }
}
impl TryFrom<Index> for WindowsRefIndexReader {
    type Error = TantivyError;

    fn try_from(index: Index) -> Result<Self, Self::Error> {
        // Register our custom Tokenizer
        index.tokenizers()
            .register("rawlower", RawLowerTokenizer);
        let index_reader = index.reader()?;
        Ok(WindowsRefIndexReader{index_reader})
    }
}


/// Handle Index writing operations such iterating file lists and indexing entries.
pub struct WindowRefIndexWriter {
    vanilla_path: PathBuf,
    index_writer: IndexWriter
}
impl WindowRefIndexWriter {
    /// Get a WindowRefIndexWriter from an index
    pub fn from_index(
        vanilla_path: impl AsRef<Path>,
        index: Index,
        memory_arena_num_bytes: usize
    ) -> Result<Self, TantivyError> {
        let vanilla_path = vanilla_path.as_ref().to_path_buf().clone();

        // Register our custom Tokenizer
        index.tokenizers()
            .register("rawlower", RawLowerTokenizer);

        let index_writer = index.writer(memory_arena_num_bytes)?;
        Ok(WindowRefIndexWriter{vanilla_path, index_writer})
    }

    /// Delete all documents in index
    pub fn delete_all_documents(&mut self, commit: bool) -> Result<(), VanillaError> {
        self.index_writer.delete_all_documents()?;

        if commit {
            self.index_writer.commit()?;
        }

        Ok(())
    }

    /// Perform MT indexing operation
    pub fn index_mt(&mut self) -> Result<(), VanillaError> {
        let file_list_iter = WinFileListIterator::from_path(
            &self.vanilla_path
        );

        let index = self.index_writer.index();
        let schema = index.schema();

        let mut actions = Vec::new();
        for (i, (location, file_list)) in file_list_iter.enumerate() {
            actions.push((i, location, file_list));
        }

        let size = actions.len();
        actions.par_iter()
            .for_each(|location_tuple|{
                if let Err(e) = index_file_list(
                    size, 
                    location_tuple,
                    &self.index_writer,
                    &schema
                ) {
                    error!("{:?}", e);
                }
            });

        self.index_writer.commit()?;

        Ok(())
    }
    
    /// Perform single threaded index operation
    pub fn index(&mut self) -> Result<(), VanillaError> {
        let set_count = get_system_info_files(&self.vanilla_path).len();
        info!("[starting] Indexing path: {} [{} data sets]", &self.vanilla_path.to_string_lossy(), set_count);

        let index = self.index_writer.index();
        let schema = index.schema();
        let file_list_iter = WinFileListIterator::from_path(&self.vanilla_path);
        for (i, (location, file_list)) in file_list_iter.enumerate() {
            let i = i + 1;

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

            info!(
                "[starting {}/{}] Indexing path: {}",
                i, set_count,
                location.to_string_lossy()
            );

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

            info!(
                "[finished {}/{}] Indexing path: {}",
                i, set_count,
                location.to_string_lossy()
            );
        }

        info!("[finished] Indexing path: {}", &self.vanilla_path.to_string_lossy());
        Ok(())
    }
}