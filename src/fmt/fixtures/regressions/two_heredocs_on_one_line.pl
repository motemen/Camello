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
