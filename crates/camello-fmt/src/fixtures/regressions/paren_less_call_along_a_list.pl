# A bareword call with no parentheses takes the rest of the list as its
# arguments, so the pairs written under it are that argument list to camello.
# Hanging them under the call's first argument measured an offset from an indent
# the call's name is nowhere near, and the lines came back two columns right of
# the list they belong to — the `=>` column agreed, the line starts did not.
my $x = (
    a   => f Str,
    bbb => g Str,
);

my $row = [
    'foo' => f Str,
    'bar' => g Str,
    'baz' => g h Str,
];

# A call that begins its line still hangs under its first argument.
warn "aaa",
     "bbb";

# And one wrapped outside a list keeps its continuation indent.
my $y = foo 1,
    2;

# Where the brackets break it makes no difference that the call was written on a
# line of its own: every element of such a list is, and what it swallowed sits
# at the same level.
my $one = (
    'x',
    f Str,
    bbb => 2,
);

# A list whose brackets do not break keeps the writer's own lines, and there a
# call after a `,` really is written along the list.
my %three = ('x', f Str,
    bbb => 2);
