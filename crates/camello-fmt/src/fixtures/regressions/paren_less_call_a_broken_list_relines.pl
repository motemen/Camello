# Inside brackets that break, every element takes a line at the brackets' level
# — including the ones a paren-less call swallowed. Deciding that from where the
# writer had put the call answered "along the list" on the first pass and "on a
# line of its own" on the second, and the lines under it moved between them: the
# layout has to be a fixed point (the formatter contract, I2).
my $foo = (
    'bar',
    f Str,
    baz => 2,
);
