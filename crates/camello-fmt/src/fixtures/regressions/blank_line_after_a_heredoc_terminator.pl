# A blank line between two statements, where the first ends in a heredoc body.
# It sits between `my $body = ...` and `check(...)`, so BLANK_LINE-2 keeps it —
# but the renderer suppresses a blank line after any verbatim line, and the
# heredoc terminator is one.
use Test2::V0;

sub t {
    my $body = <<'TEXT';
hello
TEXT

    check($body);
}
