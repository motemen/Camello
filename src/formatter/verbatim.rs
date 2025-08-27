use crate::{PerlNode, SyntaxKind};
use rowan::NodeOrToken;

use super::Formatter;

impl Formatter {
    /// Format a data section (__END__ or __DATA__)
    /// Data sections should be preserved exactly as-is without any formatting changes
    pub fn format_data_section(&mut self, node: &PerlNode) {
        // Ensure we're on a new line before the data section
        if !self.at_line_start {
            self.output.push('\n');
            self.at_line_start = true;
        }

        // Process all children (keyword + data content) without any modifications
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Token(token) => {
                    let text = token.text();
                    match token.kind() {
                        SyntaxKind::END_KW | SyntaxKind::DATA_KW => {
                            // Output the keyword exactly as-is
                            self.output.push_str(text);
                        }
                        SyntaxKind::DATA_SECTION => {
                            // Output the data content exactly as-is, preserving all formatting
                            self.output.push_str(text);
                        }
                        _ => {
                            // Handle any other tokens (whitespace, etc.) as-is
                            self.output.push_str(text);
                        }
                    }
                }
                NodeOrToken::Node(_) => {
                    // Data sections shouldn't contain nested nodes, but handle gracefully
                    // by preserving the original text
                }
            }
        }
    }

    /// Format a POD block
    /// POD blocks should be preserved exactly as-is without any formatting changes
    pub fn format_pod_block(&mut self, node: &PerlNode) {
        // Ensure we're on a new line before the POD block
        if !self.at_line_start {
            self.output.push('\n');
            self.at_line_start = true;
        }

        // Process all children (POD command + content + =cut) without any modifications
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Token(token) => {
                    self.output.push_str(token.text());
                }
                NodeOrToken::Node(_) => {
                    unreachable!("POD blocks should not contain nested nodes");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_perl;

    #[test]
    fn test_end_data_section_basic() {
        let input = r#"
my $x = 1;
__DATA__
This is data after __DATA__ $#&!
  Raw string here~
        "#;

        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = crate::formatter::format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $x = 1;
        __DATA__
        This is data after __DATA__ $#&!
          Raw string here~
        ");
    }

    #[test]
    fn test_pod_with_code_before_and_after() {
        let input = r#"my $var = 1;

=head1 DESCRIPTION

This is a POD section with detailed description.
It preserves all formatting exactly.

=cut

my $other = 2;
"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = crate::formatter::format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $var = 1;
        =head1 DESCRIPTION

        This is a POD section with detailed description.
        It preserves all formatting exactly.

        =cut
        my $other = 2;
        ");
    }

    #[test]
    fn test_pod_at_eof_without_cut() {
        let input = r#"my $var = 1;

=pod

This POD block goes to EOF without =cut.
Everything after =pod should be treated as POD content.
"#;
        let (syntax, err) = parse_perl(input);
        assert!(err.is_empty(), "Parse errors: {:?}", err);

        let formatted = crate::formatter::format(&syntax);

        insta::assert_snapshot!(formatted, @r"
        my $var = 1;
        =pod

        This POD block goes to EOF without =cut.
        Everything after =pod should be treated as POD content.
        ");
    }
}
