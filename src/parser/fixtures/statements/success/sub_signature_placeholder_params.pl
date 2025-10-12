# Placeholder parameters in signatures
sub foo($) {
    warn "one placeholder";
}

sub bar($, $x) {
    warn "placeholder and named: $x";
}

sub baz($x, $, $y) {
    warn "mixed: $x $y";
}

sub qux(@) {
    warn "array placeholder";
}

sub quux(%, $x) {
    warn "hash placeholder and named: $x";
}
