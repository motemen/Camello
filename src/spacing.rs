use crate::syntax_kind::SyntaxKind;

pub fn get_spacing(prev: Option<SyntaxKind>, current: SyntaxKind) -> &'static str {
    let needs_space = match (prev, current) {
        // 演算子の前後
        (Some(_), SyntaxKind::EQ) | (Some(SyntaxKind::EQ), _) => true,
        (Some(_), SyntaxKind::PLUS) | (Some(SyntaxKind::PLUS), _) => true,
        (Some(_), SyntaxKind::MINUS) | (Some(SyntaxKind::MINUS), _) => true,
        (Some(_), SyntaxKind::FAT_COMMA) | (Some(SyntaxKind::FAT_COMMA), _) => true,

        // Comparison operators
        (Some(_), SyntaxKind::GT) | (Some(SyntaxKind::GT), _) => true,
        (Some(_), SyntaxKind::LT) | (Some(SyntaxKind::LT), _) => true,
        (Some(_), SyntaxKind::GE) | (Some(SyntaxKind::GE), _) => true,
        (Some(_), SyntaxKind::LE) | (Some(SyntaxKind::LE), _) => true,
        (Some(_), SyntaxKind::EQ_EQ) | (Some(SyntaxKind::EQ_EQ), _) => true,
        (Some(_), SyntaxKind::NE) | (Some(SyntaxKind::NE), _) => true,

        // Regex operators
        (Some(_), SyntaxKind::REGEX_MATCH) | (Some(SyntaxKind::REGEX_MATCH), _) => true,
        (Some(_), SyntaxKind::REGEX_NOT_MATCH) | (Some(SyntaxKind::REGEX_NOT_MATCH), _) => true,

        // Exception: no space before semicolon when previous token is slash (for q-string delimiters)
        (Some(SyntaxKind::SLASH), SyntaxKind::SEMICOLON) => false,

        // Multiplicative operators (but not PERCENT which is used as sigil)
        (Some(_), SyntaxKind::STAR) | (Some(SyntaxKind::STAR), _) => true,
        (Some(_), SyntaxKind::SLASH) | (Some(SyntaxKind::SLASH), _) => true,
        (Some(_), SyntaxKind::MODULO) | (Some(SyntaxKind::MODULO), _) => true,
        (Some(_), SyntaxKind::X) | (Some(SyntaxKind::X), _) => true,

        // Logical operators
        (Some(_), SyntaxKind::LOGICAL_AND) | (Some(SyntaxKind::LOGICAL_AND), _) => true,
        (Some(_), SyntaxKind::LOGICAL_OR) | (Some(SyntaxKind::LOGICAL_OR), _) => true,

        // foo, bar
        (Some(SyntaxKind::COMMA), _) => true,
        (Some(_), SyntaxKind::COMMA) => false,

        // キーワードの後
        (
            Some(
                SyntaxKind::MY_KW
                | SyntaxKind::OUR_KW
                | SyntaxKind::STATE_KW
                | SyntaxKind::LOCAL_KW,
            ),
            _,
        ) => true,
        (Some(SyntaxKind::SUB_KW), SyntaxKind::IDENT) => true,
        (Some(SyntaxKind::SUB_KW), SyntaxKind::QUALIFIED_IDENT) => true,
        (Some(SyntaxKind::FOR_KW), _) => true,
        (Some(SyntaxKind::FOREACH_KW), _) => true,
        (Some(SyntaxKind::WHILE_KW), _) => true,
        (Some(SyntaxKind::IF_KW), _) => true,
        (Some(SyntaxKind::ELSIF_KW), _) => true,
        (Some(SyntaxKind::ELSE_KW), _) => true,
        (Some(SyntaxKind::PACKAGE_KW), _) => true,
        (Some(SyntaxKind::USE_KW), _) => true,
        (Some(SyntaxKind::RETURN_KW), _) => true,

        // Before left brace "{"
        (Some(_), SyntaxKind::L_BRACE) => true,

        // After R_BRACE, add space before expressions (for block functions) but not before semicolons
        (Some(SyntaxKind::R_BRACE), kind) if kind != SyntaxKind::SEMICOLON => true,

        // 括弧の内側はスペースなし、但し括弧の前は適切にスペースを入れる
        (Some(SyntaxKind::L_PAREN), _) => false,
        (Some(_), SyntaxKind::R_PAREN) => false,
        (Some(SyntaxKind::L_BRACE), _) => false,

        // Before L_PAREN, add space after variables and keywords (but not after identifiers or qualified identifiers for function calls)
        (Some(kind), SyntaxKind::L_PAREN)
            if kind.is_variable()
                || matches!(
                    kind,
                    SyntaxKind::MY_KW
                        | SyntaxKind::OUR_KW
                        | SyntaxKind::STATE_KW
                        | SyntaxKind::LOCAL_KW
                        | SyntaxKind::FOR_KW
                        | SyntaxKind::FOREACH_KW
                        | SyntaxKind::WHILE_KW
                        | SyntaxKind::IF_KW
                        | SyntaxKind::ELSIF_KW
                ) =>
        {
            true
        }

        // a->b
        (Some(SyntaxKind::ARROW), _) | (Some(_), SyntaxKind::ARROW) => false,

        // After identifier not followed by a semicolon, double colon, or left parenthesis
        (Some(SyntaxKind::IDENT), kind)
            if kind != SyntaxKind::SEMICOLON
                && kind != SyntaxKind::DOUBLE_COLON
                && kind != SyntaxKind::L_PAREN =>
        {
            true
        }

        // :: の前後はスペースなし（パッケージ名区切り）
        (Some(_), SyntaxKind::DOUBLE_COLON) | (Some(SyntaxKind::DOUBLE_COLON), _) => false,

        _ => false,
    };

    if needs_space {
        " "
    } else {
        ""
    }
}
