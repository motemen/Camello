use super::{spacing, Formatter, Line};
use crate::{PerlNode, SyntaxKind};

impl Formatter {
    pub(super) fn handle_newline(&mut self) {
        let line = std::mem::take(&mut self.current_line);
        self.lines.push(line);
        self.at_line_start = true;
    }

    pub(super) fn add_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.current_line.text.push_str(&self.indent_string);
        }
    }

    pub(super) fn handle_spacing_before(&mut self, current: SyntaxKind) {
        let context = spacing::SpacingContext {
            prev_token: self.prev_token_kind,
            current_token: current,
            at_line_start: self.at_line_start,
        };

        if spacing::needs_space_before(&context) {
            self.write_char(' ');
        }
    }

    pub(super) fn add_empty_line_before_if_needed(&mut self, node: &PerlNode) {
        // Don't add automatic empty lines if we already have pending empty lines from source
        // The pending empty lines will be output naturally when the next non-trivia token is processed
        if self.pending_empty_lines > 0 {
            return;
        }

        // Add an empty line if the previous sibling is of a different type,
        // or if this is a SUB_DEF with any preceding sibling (to separate all subs)
        // Exception: Don't add empty line between PACKAGE_STMT and USE_STMT/NO_STMT
        if let Some(prev) = node.prev_sibling() {
            let should_add_empty_line = match node.kind() {
                // For SUB_DEF, always add empty line if there's a preceding sibling
                SyntaxKind::SUB_DEF => true,
                // For regular statements, don't add automatic empty lines
                // They should only get empty lines if they were in the source
                SyntaxKind::STMT
                | SyntaxKind::LABELED_STMT
                | SyntaxKind::DECLARATION_STMT
                | SyntaxKind::ELLIPSIS_STMT => false,
                // For other node types, use the original logic
                _ => {
                    prev.kind() != node.kind()
                        && !(prev.kind() == SyntaxKind::PACKAGE_STMT
                            && (node.kind() == SyntaxKind::USE_STMT
                                || node.kind() == SyntaxKind::NO_STMT))
                }
            };

            if should_add_empty_line {
                self.add_empty_line_before();
            }
        }
    }

    pub(super) fn add_empty_line_after_if_needed(&mut self, node: &PerlNode) {
        // Check if the next node already has empty lines from source whitespace
        // If so, skip automatic insertion
        if let Some(_next) = node.next_sibling() {
            // Look for whitespace tokens between this node and the next
            if let Some(last_token) = node.last_token() {
                let mut current = last_token.next_token();
                let mut total_newlines = 0;

                // Count newlines across consecutive trivia tokens
                while let Some(token) = current {
                    match token.kind() {
                        SyntaxKind::NEWLINE => {
                            total_newlines += 1;
                            current = token.next_token();
                        }
                        SyntaxKind::WHITESPACE | SyntaxKind::COMMENT => {
                            current = token.next_token();
                        }
                        _ => break,
                    }
                }

                // If there are already multiple newlines (indicating empty lines), don't add more
                if total_newlines > 1 {
                    return;
                }
            }
        }

        // Add an empty line if the next sibling is of a different type.
        // Exception: Don't add empty line between PACKAGE_STMT and USE_STMT/NO_STMT
        if let Some(next) = node.next_sibling() {
            if next.kind() != node.kind() {
                // Don't add empty line between PACKAGE_STMT and USE_STMT/NO_STMT
                if !(node.kind() == SyntaxKind::PACKAGE_STMT
                    && (next.kind() == SyntaxKind::USE_STMT || next.kind() == SyntaxKind::NO_STMT))
                {
                    self.add_empty_line_after();
                }
            }
        }
    }

    pub(super) fn add_empty_line_before(&mut self) {
        // Only add empty line if this is not the first node and we don't already have one
        if !self.is_output_empty() && !self.ends_with_double_newline() {
            if !self.ends_with_newline() {
                self.handle_newline();
            }
            self.lines.push(Line::new());
            self.at_line_start = true;
        }
    }

    pub(super) fn add_empty_line_after(&mut self) {
        // Force at least one empty line after the node
        if !self.ends_with_newline() {
            self.handle_newline();
        }
        // Add one more newline to create an empty line
        if !self.ends_with_double_newline() {
            self.lines.push(Line::new());
        }
    }

    /// Output pending empty lines when appropriate
    pub(super) fn output_pending_empty_lines(&mut self) {
        if self.pending_empty_lines > 0 {
            // Ensure we're on a new line first
            if !self.ends_with_newline() {
                self.handle_newline();
            }
            // Add empty lines
            for _ in 0..self.pending_empty_lines {
                self.lines.push(Line::new());
            }
            self.pending_empty_lines = 0;
            self.at_line_start = true;
        }
    }
}
