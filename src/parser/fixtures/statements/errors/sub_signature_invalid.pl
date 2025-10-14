sub bad_number ($1) {
    return;
}

sub bad_digits ($123) {
    return;
}

sub bad_dash ($-foo) {
    return;
}

sub bad_low_precedence ($x = 1 or 2) {
    return $x;
}
