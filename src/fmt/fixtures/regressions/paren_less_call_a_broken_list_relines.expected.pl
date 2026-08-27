# `f Str` is written after a `,` on a line it shares, and the brackets break, so
# the formatter is about to give it a line of its own. Asking the input where it
# sat answered "along the list" on the first pass and "on its own line" on the
# second, and the lines under it moved between them: the layout has to be a
# fixed point (the formatter contract, I2).
my $foo = (
    'bar',
    f Str,
      baz => 2,
);
