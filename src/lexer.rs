use crate::SyntaxKind;
use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip r"[ \t\f]+")] 
pub enum Token {
    // Sigils（変数の型を示すプレフィックス）
    #[token("$")]
    Dollar,
    
    #[token("@")]
    At,
    
    
    // 識別子（サブルーチン名など）
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,
    
    // リテラル
    #[regex(r"[0-9]+(\.[0-9]+)?")]
    Number,
    
    #[regex(r#""([^"\\]|\\.)*""#)]
    #[regex(r"'([^'\\]|\\.)*'")]
    String,
    
    // 記号
    #[token("{")]
    LBrace,
    
    #[token("}")]
    RBrace,
    
    #[token("(")]
    LParen,
    
    #[token(")")]
    RParen,
    
    #[token(";")]
    Semicolon,
    
    #[token(",")]
    Comma,
    
    // 演算子
    #[token("=")]
    Eq,
    
    #[token("+")]
    Plus,
    
    #[token("-")]
    Minus,
    
    #[token("=>")]
    Arrow,
    
    // Multiplicative operators
    #[token("*")]
    Star,
    
    #[token("/")]
    Slash,
    
    #[token("%")]
    Percent,
    
    
    // 改行（重要なので個別にトークン化）
    #[regex(r"\r\n|\r|\n")]
    Newline,
    
    // コメント
    #[regex(r"#[^\r\n]*")]
    Comment,
}

impl Token {
    pub fn to_syntax_kind(&self) -> SyntaxKind {
        match self {
            Token::Dollar => SyntaxKind::DOLLAR,
            Token::At => SyntaxKind::AT,
            Token::Percent => SyntaxKind::PERCENT,
            Token::Ident => SyntaxKind::IDENT,
            Token::Number => SyntaxKind::NUMBER,
            Token::String => SyntaxKind::STRING,
            Token::LBrace => SyntaxKind::L_BRACE,
            Token::RBrace => SyntaxKind::R_BRACE,
            Token::LParen => SyntaxKind::L_PAREN,
            Token::RParen => SyntaxKind::R_PAREN,
            Token::Semicolon => SyntaxKind::SEMICOLON,
            Token::Comma => SyntaxKind::COMMA,
            Token::Eq => SyntaxKind::EQ,
            Token::Plus => SyntaxKind::PLUS,
            Token::Minus => SyntaxKind::MINUS,
            Token::Arrow => SyntaxKind::ARROW,
            Token::Star => SyntaxKind::STAR,
            Token::Slash => SyntaxKind::SLASH,
            Token::Newline => SyntaxKind::WHITESPACE,
            Token::Comment => SyntaxKind::COMMENT,
        }
    }
}

pub struct Lexer<'a> {
    logos_lexer: logos::Lexer<'a, Token>,
    prev_kind: Option<SyntaxKind>,
    second_prev_kind: Option<SyntaxKind>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let logos_lexer = Token::lexer(input);
        
        Self { 
            logos_lexer,
            prev_kind: None,
            second_prev_kind: None,
        }
    }
    
    pub fn next_token(&mut self) -> Option<(SyntaxKind, &'a str)> {
        match self.logos_lexer.next() {
            Some(Ok(token)) => {
                let text = self.logos_lexer.slice();
                let syntax_kind = self.disambiguate(token, text);
                
                // Update prev_kind only for non-trivia tokens
                if !syntax_kind.is_trivia() {
                    self.second_prev_kind = self.prev_kind;
                    self.prev_kind = Some(syntax_kind);
                }
                
                Some((syntax_kind, text))
            }
            Some(Err(_)) => {
                // エラートークンとして処理
                let text = self.logos_lexer.slice();
                Some((SyntaxKind::ERROR, text))
            }
            None => None,
        }
    }
    
    fn disambiguate(&self, token: Token, text: &str) -> SyntaxKind {
        match token {
            Token::Ident => {
                // 識別子の場合、キーワードかどうかチェック
                match text {
                    "sub" => SyntaxKind::SUB_KW,
                    "my" => SyntaxKind::MY_KW,
                    "if" => SyntaxKind::IF_KW,
                    "else" => SyntaxKind::ELSE_KW,
                    "x" => self.disambiguate_x(),
                    _ => SyntaxKind::IDENT,
                }
            }
            Token::Percent => {
                // % の場合、文脈によって sigil か modulo operator かを判定
                self.disambiguate_percent()
            }
            _ => token.to_syntax_kind(),
        }
    }

    fn disambiguate_percent(&self) -> SyntaxKind {
        // Check previous context using prev_kind
        let has_sigil_before = self.prev_kind.map_or(false, |k| k.is_sigil());
        
        // Check if we're in a context that suggests variable declaration
        // e.g., IDENT preceded by a sigil (like "@array" where "array" follows "@")
        let prev_ident_has_sigil = matches!(self.prev_kind, Some(SyntaxKind::IDENT)) 
            && self.second_prev_kind.map_or(false, |k| k.is_sigil());
        
        // Check if previous token is a standalone value (not part of a sigil+ident pair)
        let has_standalone_operand = match self.prev_kind {
            Some(SyntaxKind::IDENT) => !prev_ident_has_sigil, // IDENT that's not preceded by sigil
            Some(SyntaxKind::NUMBER | SyntaxKind::R_PAREN | SyntaxKind::R_BRACE) => true,
            _ => false,
        };
        
        // Check next token efficiently using lookahead
        let next_is_ident = self.peek_next_token() == Some(Token::Ident);
        
        // Disambiguation logic:
        // 1. If previous token is a sigil, then % is also a sigil (variable list like "$scalar @array %hash")
        if has_sigil_before {
            SyntaxKind::PERCENT
        }
        // 2. If previous IDENT was preceded by sigil, we're likely in variable list (like "@array %hash")
        else if prev_ident_has_sigil && next_is_ident {
            SyntaxKind::PERCENT
        }
        // 3. If we have a standalone operand before AND an identifier after, it's modulo (like "c % d")
        else if has_standalone_operand && next_is_ident {
            SyntaxKind::MODULO
        }
        // 4. If % is followed by an identifier, default to sigil (like "my %hash")
        else if next_is_ident {
            SyntaxKind::PERCENT
        }
        // 5. Default to operator
        else {
            SyntaxKind::MODULO
        }
    }

    fn disambiguate_x(&self) -> SyntaxKind {
        // "x" is identifier if it follows a sigil, otherwise it's repetition operator
        if self.prev_kind.map_or(false, |k| k.is_sigil()) {
            SyntaxKind::IDENT
        } else {
            SyntaxKind::X
        }
    }
    
    fn peek_next_token(&self) -> Option<Token> {
        // Efficient lookahead using a clone of the logos lexer
        let mut lookahead = self.logos_lexer.clone();
        match lookahead.next() {
            Some(Ok(token)) => Some(token),
            _ => None,
        }
    }
    
    pub fn span(&self) -> std::ops::Range<usize> {
        self.logos_lexer.span()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let mut lexer = Lexer::new("my $var = 1;");
        
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "var")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::EQ, "=")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::NUMBER, "1")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SEMICOLON, ";")));
        assert_eq!(lexer.next_token(), None);
    }
    
    #[test]
    fn test_keywords() {
        let mut lexer = Lexer::new("sub my if else");
        
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SUB_KW, "sub")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IF_KW, "if")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::ELSE_KW, "else")));
    }
    
    #[test]
    fn test_variables() {
        let mut lexer = Lexer::new("$scalar @array %hash");
        
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "scalar")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::AT, "@")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "array")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::PERCENT, "%")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "hash")));
    }

    #[test]
    fn test_hash_arrow() {
        let mut lexer = Lexer::new("a => 1");
        
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "a")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::ARROW, "=>")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::NUMBER, "1")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_multiplicative_operators() {
        let mut lexer = Lexer::new("a * b / c % d x 3");
        
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "a")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::STAR, "*")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "b")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SLASH, "/")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "c")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MODULO, "%")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "d")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::X, "x")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::NUMBER, "3")));
        assert_eq!(lexer.next_token(), None);
    }

    #[test]
    fn test_sigil_tokens() {
        let mut lexer = Lexer::new("$ @ %");
        
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::AT, "@")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::PERCENT, "%")));
        assert_eq!(lexer.next_token(), None);
    }

}