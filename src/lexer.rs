use crate::SyntaxKind;
use logos::Logos;

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[ \t\f]+")] 
pub enum Token {
    // Sigils（変数の型を示すプレフィックス）
    #[token("$")]
    Dollar,
    
    #[token("@")]
    At,
    
    #[token("%", priority = 1)]
    Percent,
    
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
    
    #[token("%", priority = 2)]
    Modulo,
    
    
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
            Token::Modulo => SyntaxKind::MODULO,
            Token::Newline => SyntaxKind::WHITESPACE,
            Token::Comment => SyntaxKind::COMMENT,
        }
    }
}

pub struct Lexer<'a> {
    logos_lexer: logos::Lexer<'a, Token>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let logos_lexer = Token::lexer(input);
        
        // キーワードの判定ロジックを追加
        // Logosはcontextual keywordsに対応していないため、手動で処理
        
        Self { logos_lexer }
    }
    
    pub fn next_token(&mut self) -> Option<(SyntaxKind, &'a str)> {
        match self.logos_lexer.next() {
            Some(Ok(token)) => {
                let text = self.logos_lexer.slice();
                let syntax_kind = self.resolve_keyword_or_ident(token, text);
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
    
    fn resolve_keyword_or_ident(&self, token: Token, text: &str) -> SyntaxKind {
        match token {
            Token::Ident => {
                // 識別子の場合、キーワードかどうかチェック
                match text {
                    "sub" => SyntaxKind::SUB_KW,
                    "my" => SyntaxKind::MY_KW,
                    "if" => SyntaxKind::IF_KW,
                    "else" => SyntaxKind::ELSE_KW,
                    "x" => SyntaxKind::X, // 文脈によって演算子として扱う
                    _ => SyntaxKind::IDENT,
                }
            }
            _ => token.to_syntax_kind(),
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