# A bareword call with a block takes the rest of the list as its arguments, the
# same as one with any other first argument. Hanging what it swallowed under
# that first argument asks for a column, and a block written across lines has
# none to give: `bar {` came back four columns right of the `}` that closes it,
# with its own body and brace where they always were.
my $foo = [
    bar {
        1;
    },
    baz {
        2;
    },
];

# The same with something between them.
my $qux = [
    bar {
        1;
    },
    2,
    baz {
        3;
    },
];
