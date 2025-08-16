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
                    "x" => self.resolve_x_context(), // 文脈によって演算子か識別子かを判定
                    _ => SyntaxKind::IDENT,
                }
            }
            Token::Percent => {
                // % の場合、文脈によって sigil か modulo operator かを判定
                self.resolve_percent_context()
            }
            _ => token.to_syntax_kind(),
        }
    }

    fn resolve_percent_context(&self) -> SyntaxKind {
        // より洗練されたヒューリスティック：
        // 1. 前に sigil (@, $) がある場合は sigil (例: "$var @arr %hash")
        // 2. 前に演算子がある場合で次に識別子がある場合は sigil
        // 3. 前に値 (識別子、数値等) があり、次にも値がある場合は operator
        
        let current_pos = self.logos_lexer.span().start;
        let source = self.logos_lexer.source();
        
        // 前のトークンをチェック（複数トークン見る）
        let mut has_value_before = false;
        let mut has_sigil_before = false;
        
        if current_pos > 0 {
            let before = &source[..current_pos];
            let trimmed_before = before.trim_end();
            if !trimmed_before.is_empty() {
                let mut temp_lexer = Token::lexer(trimmed_before);
                let mut tokens = Vec::new();
                while let Some(Ok(token)) = temp_lexer.next() {
                    tokens.push(token);
                }
                
                if let Some(last_token) = tokens.last() {
                    match last_token {
                        Token::At | Token::Dollar | Token::Percent => {
                            has_sigil_before = true;
                        }
                        Token::Ident | Token::Number | Token::RParen | Token::RBrace => {
                            has_value_before = true;
                            
                            // 識別子の前に sigil があるかチェック
                            if tokens.len() >= 2 {
                                if let Some(second_last) = tokens.get(tokens.len() - 2) {
                                    match second_last {
                                        Token::At | Token::Dollar | Token::Percent => {
                                            // sigil + ident は完全な変数なので値として扱う
                                            has_value_before = true;
                                            has_sigil_before = false;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        
        // 次のトークンをチェック
        let after_pos = self.logos_lexer.span().end;
        let remaining = &source[after_pos..];
        let trimmed = remaining.trim_start();
        
        if trimmed.is_empty() {
            return SyntaxKind::PERCENT; // ファイル終端は sigil
        }
        
        let mut temp_lexer = Token::lexer(trimmed);
        let (next_is_ident, next_is_sigil) = match temp_lexer.next() {
            Some(Ok(Token::Ident)) => (true, false),
            Some(Ok(Token::Dollar | Token::At | Token::Percent)) => (false, true),
            _ => (false, false),
        };
        
        
        // 特別なケース：複数の変数宣言の場合は sigil として扱う
        // "$scalar @array %hash" のようなパターンを検出
        let is_variable_list = source.chars().filter(|&c| c == '@' || c == '$').count() >= 2;
        
        // 判定ロジック：
        if has_sigil_before {
            SyntaxKind::PERCENT // sigil の後は sigil
        } else if is_variable_list && next_is_ident {
            SyntaxKind::PERCENT // 複数変数宣言では sigil
        } else if has_value_before && (next_is_ident || next_is_sigil) {
            SyntaxKind::MODULO // 値 % 値 のパターンは modulo operator
        } else if next_is_ident {
            SyntaxKind::PERCENT // % + identifier は sigil
        } else {
            SyntaxKind::MODULO // それ以外は operator
        }
    }

    fn resolve_x_context(&self) -> SyntaxKind {
        // "x" が sigil の直後にある場合は識別子、そうでなければ演算子
        let current_pos = self.logos_lexer.span().start;
        let source = self.logos_lexer.source();
        
        // 前のトークンをチェック
        if current_pos > 0 {
            let before = &source[..current_pos];
            let trimmed_before = before.trim_end();
            if !trimmed_before.is_empty() {
                let mut temp_lexer = Token::lexer(trimmed_before);
                let mut last_token = None;
                while let Some(Ok(token)) = temp_lexer.next() {
                    last_token = Some(token);
                }
                
                // 直前が sigil なら identifier
                if let Some(token) = last_token {
                    match token {
                        Token::At | Token::Dollar | Token::Percent => {
                            return SyntaxKind::IDENT;
                        }
                        _ => {}
                    }
                }
            }
        }
        
        // それ以外は repetition operator
        SyntaxKind::X
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