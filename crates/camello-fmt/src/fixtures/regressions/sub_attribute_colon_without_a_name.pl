# A `:` between the sub name and its body, with no attribute after it. perl
# accepts the empty attribute list, and a codebase that writes `sub f : Tests`
# everywhere acquires one of these by a dropped word.
sub foo : {
    return [ 1, 2, 3 ];
}

sub bar : Tests {
    return [ 4, 5, 6 ];
}
