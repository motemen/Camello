package Order;
use strict;
use warnings;

# The same package and the same sub, in an old checkout left inside the
# workspace. The walk sorts it before `lib/`, so the global name index answers
# `Order::charge` with this one — and hover over the *other* file's `$label`
# used to change from `Str` to `Any` the moment the index arrived.
sub charge {
    my ($class, $label) = @_;
    return $label;
}

1;
