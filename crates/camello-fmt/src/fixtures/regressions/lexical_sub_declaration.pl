# `my sub NAME { ... }`: a lexically scoped named subroutine. `our` and `state`
# take the same shape, and the declaration keyword is followed by `sub` rather
# than by a variable.
my sub helper {
    my ($x) = @_;
    return $x + 1;
}

our sub shared { return 2 }

state sub kept { return 3 }
