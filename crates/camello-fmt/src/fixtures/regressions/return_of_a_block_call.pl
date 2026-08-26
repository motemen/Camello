# `return` followed by a call whose first argument is a block. `return do { }`,
# `return sub { }`, `return eval { }` and `return grep { } @list` all parse; a
# user-defined `&`-prototyped name in the same position is the case that does
# not.
sub with_catch {
    my ($thing) = @_;
    return try {
        return $thing->fetch;
    } catch {
        return undef;
    };
}

sub without_catch {
    my ($thing) = @_;
    return try { $thing->fetch };
}
