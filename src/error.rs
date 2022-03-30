use tantivy::TantivyError;


#[derive(Debug)]
pub struct VanillaError {
    message: String
}
impl VanillaError {
    pub fn from_message(message: String) -> Self {
        Self { message }
    }
}

impl From<TantivyError> for VanillaError {
    fn from(err: TantivyError) -> Self {
        Self { message: format!("{:?}", err) }
    }
}

impl From<std::io::Error> for VanillaError {
    fn from(err: std::io::Error) -> Self {
        Self { message: format!("{:?}", err) }
    }
}