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
        // Check next token efficiently using lookahead
        let next_is_ident = self.peek_next_token() == Some(Token::Ident);
        
        // Disambiguation logic:
        // 1. If previous token is a sigil, then % is also a sigil (variable list like "$scalar @array %hash")
        if self.prev_kind.map_or(false, |k| k.is_sigil()) {
            SyntaxKind::PERCENT
        }
        // 2. If we have an IDENT before AND an IDENT after, check the context more carefully
        else if matches!(self.prev_kind, Some(SyntaxKind::IDENT)) && next_is_ident {
            // Check if this IDENT was part of a variable declaration context
            let prev_ident_has_sigil = self.second_prev_kind.map_or(false, |k| k.is_sigil());
            
            // In a variable list like "@array %hash", the "@" sigil would be second_prev_kind
            // and "array" would be prev_kind, and we want % to be a sigil
            // But in arithmetic like "$var % other_var", the "$" sigil would be second_prev_kind
            // and "var" would be prev_kind, and we want % to be modulo
            
            // The key distinction: in variable lists, all variables are at the same "level"
            // In arithmetic, one variable is the operand for an operation
            
            // If the previous IDENT is preceded by a sigil, we need to distinguish:
            // - If it's a variable list context: "@array %hash" -> % is sigil  
            // - If it's an arithmetic context: "$var % other" -> % is modulo
            
            // Heuristic: If the sigil was DOLLAR, it's more likely to be arithmetic
            // If it was AT, it's more likely to be a variable list
            if prev_ident_has_sigil {
                match self.second_prev_kind {
                    Some(SyntaxKind::DOLLAR) => SyntaxKind::MODULO, // $var % other -> modulo
                    Some(SyntaxKind::AT) => SyntaxKind::PERCENT,    // @array %hash -> sigil
                    _ => SyntaxKind::PERCENT, // Default to sigil for safety
                }
            } else {
                // No sigil before the IDENT, so it's likely arithmetic: "c % d"
                SyntaxKind::MODULO
            }
        }
        // 3. If % is followed by an identifier, default to sigil (like "my %hash")
        else if next_is_ident {
            SyntaxKind::PERCENT
        }
        // 4. Default to operator
        else {
            SyntaxKind::MODULO
        }
    }

    fn disambiguate_x(&self) -> SyntaxKind {
        // "x" is identifier if it follows a sigil or SUB_KW, otherwise it's repetition operator
        if self.prev_kind.map_or(false, |k| k.is_sigil() || k == SyntaxKind::SUB_KW) {
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

    #[test]
    fn test_percent_modulo_vs_sigil() {
        // Test the critical case mentioned by Gemini: $var % other_var should be modulo
        let mut lexer = Lexer::new("$var % other_var");
        
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "var")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::MODULO, "%"))); // Should be MODULO, not PERCENT
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "other_var")));
    }

    #[test]
    fn test_x_after_sub_keyword() {
        // Test the case mentioned by Gemini: sub x { ... } where x should be IDENT
        let mut lexer = Lexer::new("sub x {");
        
        assert_eq!(lexer.next_token(), Some((SyntaxKind::SUB_KW, "sub")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "x"))); // Should be IDENT, not X
        assert_eq!(lexer.next_token(), Some((SyntaxKind::L_BRACE, "{")));
    }

    #[test]
    fn test_array_hash_variable_list() {
        // Test that "@array %hash" correctly identifies % as sigil (not modulo)
        let mut lexer = Lexer::new("@array %hash");
        
        assert_eq!(lexer.next_token(), Some((SyntaxKind::AT, "@")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "array")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::PERCENT, "%"))); // Should be PERCENT (sigil)
        assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "hash")));
    }

}