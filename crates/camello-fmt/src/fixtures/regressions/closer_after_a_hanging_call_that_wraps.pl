# A closing bracket keeps the line it was written on where the contents in front
# of it wrap. A call written without parentheses hangs its arguments, and a wrap
# inside those is still a wrap of the contents: the `)` stays where `f($a, ...)`
# would keep it.
f(g $a,
    $b
);

f($a,
    $b
);

my @x = (h $a,
    $b
);
