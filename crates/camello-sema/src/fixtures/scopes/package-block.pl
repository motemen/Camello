use strict;
use warnings;

# `package Inner { ... }` holds its own subs, and a call written inside it
# means Inner's; the package around it is back in effect after the block.
package Outer;
sub helper { my $x = shift; return $x }

package Inner {
    sub helper { my ($a, $b) = @_; return $a . $b }
    sub run { return helper(1, 2) }
}

helper(1, 2);                   #~ warning arity: takes at most 1 argument; 2 passed

1;
