package Order;
use strict;
use warnings;

# An old checkout left inside the tree declares the same package and the same
# sub, without the annotations the file under `lib/` has grown since. It sorts
# before `lib/`, so it is what the global name index answers `Order::charge`
# with — and what `lib/Order.pm` used to be typed from.
sub charge {
    my ($class, $label) = @_;
    return $label;
}

1;
