# Two heredoc bodies follow one another, and the second starts on the line
# *after* the first terminator. Leaving that line terminator to the ordinary
# scanner made it the first byte of the second body, so B arrived as "\ntwo\n".
#
# The token stream cannot see it: HEREDOC_CONTENT holds the same text either
# way and only its boundaries move. perl can, and did.
foo(<<A, <<B);
one
A
two
B
print <<~X, <<'Y', "tail\n";
    indented
    X
literal $notinterpolated
Y
my $only = <<Z;
a single body still leaves its newline to the scanner
Z
# Two statements sharing a line with a marker must stay on that line: a body
# begins on the line *after* its marker's line (ADR 0007 §7), so putting the
# second statement on a line of its own makes it the first line of the body.
my $prog = <<'P'; warn "compiled" if $ENV{DEBUG};
print "hello\n";
P
# And no blank line may be inserted between the marker's line and the body —
# the blank line the definition rule wants after a `sub` would land inside the
# string, and the string would gain a line on every pass.
sub builtin_data { return <<'DATA' }

first line of the body is blank on purpose
DATA
print $prog;
