# A bracket opened mid-line is placed from the line it begins on (INDENT-4), so
# its closing delimiter comes back to that line's level. Where the list it is an
# element of goes on below it, that level is one to the left of the elements
# around it: the `},` closing an argument sat four columns inside of the
# argument written under it, and read as closing the call instead.
sub f {
    return frobnicate('x', {
            k => 1,
        },
        { k2 => 'v2' });
}

sub g {
    return frobnicate('x', {
            k => 1,
        },
        $alpha,
        $beta);
}

# Siblings written on the closing delimiter's own line are not below it and
# have nothing to disagree with, so the bracket stays where INDENT-4 puts it.
# This is the common shape, and pushing it in would be a level for nothing.
install({
    read  => "$sitearch/auto/$FULLEXT/.packlist",
    write => "$installsitearch/auto/$FULLEXT/.packlist",
}, 1, 0, 0);

# Nothing follows at all: the same answer, for the same reason.
is(foo, [
    object {
        call foo => 1;
    },
]);

# The list the bracket belongs to is its own. A closing delimiter that ends its
# list ends the line it is on, and what is written below belongs to whatever the
# list itself is an element of.
$obj->meth(q[
    foo
], {
    k => 1,
},
);
