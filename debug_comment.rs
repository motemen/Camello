use crate::comment_ownership::CommentAnalyzer;
use crate::parse_perl;
use crate::SyntaxKind;
use rowan::NodeOrToken;

fn main() {
    let src = "print $var; # debug output";
    let (syntax, err) = parse_perl(src);
    if !err.is_empty() {
        println!("Parse errors: {:?}", err);
    }
    
    let analyzer = CommentAnalyzer::analyze(&syntax);
    let comment_token = syntax
        .descendants_with_tokens()
        .find_map(|el| match el {
            NodeOrToken::Token(t) if t.kind() == SyntaxKind::COMMENT => Some(t),
            _ => None,
        });
        
    if let Some(token) = comment_token {
        println!("Comment text: {:?}", token.text());
        println!("Analysis result: {:?}", analyzer.ownership.get(&token));
    } else {
        println!("No comment found");
    }
}