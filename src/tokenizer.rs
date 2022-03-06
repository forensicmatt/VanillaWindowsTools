use tantivy::tokenizer::{Token, TokenStream, Tokenizer};
use tantivy::tokenizer::BoxTokenStream;

/// Using boiler plate for raw tokenizer
/// https://docs.rs/tantivy/latest/src/tantivy/tokenizer/raw_tokenizer.rs.html#6

/// This tokenizer creates a single raw token for the entire value
/// and uses lowercasing so that case insensitive searches can be
/// performed.
#[derive(Clone)]
pub struct RawLowerTokenizer;

pub struct RawLowerTokenStream {
    token: Token,
    has_token: bool,
}

/// Implement custom functionality for our RawLowerTokenizer
impl Tokenizer for RawLowerTokenizer {
    fn token_stream<'a>(&self, text: &'a str) -> BoxTokenStream<'a> {
        let token = Token {
            offset_from: 0,
            offset_to: text.len(),
            position: 0,
            text: text.to_lowercase(),
            position_length: 1,
        };
        RawLowerTokenStream {
            token,
            has_token: true,
        }
        .into()
    }
}

impl TokenStream for RawLowerTokenStream {
    fn advance(&mut self) -> bool {
        let result = self.has_token;
        self.has_token = false;
        result
    }

    fn token(&self) -> &Token {
        &self.token
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.token
    }
}