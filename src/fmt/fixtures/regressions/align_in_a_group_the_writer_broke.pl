# A group is seeded broken by a newline straight after the opening bracket, and
# `f($o,` has something there instead — so it is flat, and nothing inside it
# takes a line break of the formatter's. The lines the writer went on to write
# are still several lines, and their `=>` is a column like any other; the
# anchors were dropped on the strength of the seed and the table was the one
# camello would not lay out.
f($o,
    foo    => 1,
    bazbaz => 2,
);

# Which is what the same call gets when the bracket does seed a break.
f(
    $o,
    foo    => 1,
    bazbaz => 2,
);

# A group that really does stay on one line still holds nothing: the `=>` of a
# one-line call must not join the vertical group of the statements around it.
sub t {
    my $one   = foo(alpha => $x);
    my $two   = bar(b => $y, charlie => $z);
    my $three = 1;
}

# Nor may the `=` inside a one-line paren join the assignments beside it.
sub u {
    ($self->{FOO} = $self->{BAR}) =~ s{::}{-}g unless $self->{FOO};
    $self->{FOOBAR} ||= 1;
}

# A trailing comment is not in the group at all — it is at the end of the line,
# whichever token it happens to trail.
my @dirs = defined $alt
    ? File::Spec->splitdir($dirs)           # a local-OS path
    : File::Spec::Unix->splitdir($dirs);    # UNIX-style, likely
