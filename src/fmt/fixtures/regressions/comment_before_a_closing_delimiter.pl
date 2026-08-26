# An own-line comment written just before a closing bracket or brace takes the
# indentation of where it is (COMMENT-2), and where it is, is inside: it is the
# last line of the contents, not the first of what the delimiter closes onto.
# Emitted with the delimiter it came back at the enclosing level, so a pair of
# comments written around a construct came back at two different columns.
my $foo = (
    bar => 1,
    # inside the parentheses
);

sub foo {
    my $bar = 1;
    # inside the block
}

my $baz = [
    1,
    # inside the brackets
];

my @qux = @{
    $foo
    # inside the dereference
};

if ($foo) {
    bar();
    # inside the branch
} else {
    baz();
    # inside the other branch
}

my $quux = (
    # nothing else inside at all
);
