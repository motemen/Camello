# A bareword written as the value of a pair stops before the next pair, so the
# pairs under it are elements of the list they were written in and the brackets
# put one on each line (INDENT-2). Kept in the call's argument list they were
# out of that rule's reach, and only the line the writer had already given them
# saved them.
my $foo = (
    bar => f Str,
    baz => 2,
    qux => 3,
);

# Not the value of a pair: the call keeps the whole list, which is what an
# option table wants.
die $usage
    unless getopt \@args,
       'b|backlog=i' => sub { 1 },
       'c|clients=i' => sub { 2 };

# A builtin is not guessed about at all.
warn "aaa",
     bbb => 1;

# A key the lookahead cannot see is a key it does not act on.
my $two = (
    bar => f Str,
    $k  => 2,
);
