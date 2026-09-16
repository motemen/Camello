# A `try` statement takes no modifier, so a modifier makes this a call to a
# function named `try` (Try::Tiny) written as an expression statement.
try {
    do_something();
} if $enabled;

try {
    risky();
} catch {
    handle_error($_);
} unless $skip;

try { step($_) } for @items;

# The same two tokens begin a statement of its own. The block after the
# condition is what tells them apart.
try {
    risky();
} catch ($e) {
    warn $e;
}
if ($retry) {
    again();
}

try {
    risky();
}
for my $item (@items) {
    process($item);
}
