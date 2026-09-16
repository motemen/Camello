# A `try` statement takes no modifier, so a modifier makes this a call to a
# function named `try` (Try::Tiny). Read as the statement it is not, the `if`
# was taken for the head of one and the parse failed at the missing `(`.
try { foo() } if 1;

try { risky() } catch { handle($_) } unless $skip;

try { step($_) } for @items;

# The same two tokens begin a statement of its own, and it keeps its own shape.
try {
    risky();
}
if ($retry) {
    again();
}
