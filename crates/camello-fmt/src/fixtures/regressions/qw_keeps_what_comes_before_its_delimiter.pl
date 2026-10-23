# What is written between `qw` and its opening delimiter stays as written,
# as it does for `q [...]` and the other quote-like operators: a space, or a
# comment, which runs to the end of its line. The contents are indented all the
# same (INDENT-2).
my @spaced = qw (
FOO
BAR
);

my @commented = qw # the words
(
FOO
BAR
);
