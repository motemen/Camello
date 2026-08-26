# A bracket's contents are placed from the bracket. The call the bracket was
# written in hangs its own arguments from a column of its own, and the contents
# know nothing about that column: the elements the first `object` swallowed came
# back three columns right of the one it did not.
is foo, [
    object {
        prop blessed => 'C';
        call foo     => 1;
    },
    object {
        prop blessed => 'C';
        call foo     => 1;
    },
];

# Written with the call's own parentheses there is no column to hang from, and
# what carried the level away was the wrap inside the brackets: it was taken
# once for the whole of the contents rather than for the element it was in, so
# every line after the first element came back a level deeper and the `]`
# closing them with it.
is(foo, [
    object {
        prop blessed => 'C';
        call foo     => 1;
    },
    object {
        prop blessed => 'C';
        call foo     => 1;
    },
]);

# A wrap inside a bracket still takes its level, and hands it back at the
# element it was written in.
f(
    $a
        + $b,
    $c,
);

# A filehandle is placed beside the name, so the call swallowed nothing and the
# lines under it are its own arguments wrapping.
unless (
    print $fh "a",
        $b,
) {
    warn "no";
}

# A bracket opened on a line the call hung is placed from there too (INDENT-4):
# one level in from the bracket, and the closing bracket back at it. Measured
# from the statement's level instead, the contents came back to the left of the
# `[` holding them and the `]` landed in column zero.
foo "bar",
    [
        1,
    ],
    "baz";

foo "bar",
    {
        k => 1,
    },
    "baz";

# A line the renderer did not place says nothing about where what opens on it
# belongs: verbatim content owns its own lines and starts them in column 0, so
# the bracket written after one is still placed from the argument list it is in.
sub verbatim_before_a_bracket {
    $obj->meth(q[
        foo
    ], {
        k => 1,
    },
    );
}
