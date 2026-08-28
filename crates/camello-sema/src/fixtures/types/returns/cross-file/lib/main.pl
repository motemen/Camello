use strict;
use warnings;
use Top;
use Outer;
use Ping;

# The chain resolved: three rounds of tier 2, and the type at the end of it is
# the one the bottom of the chain built.
Top->get->id;
Top->get->nope;                 #~ warning unknown-method: `Row` declares no method `nope`

# The annotation in the middle is what the top of that chain carries.
Outer->via_guard->id;           #~ warning maybe-deref: may be undefined here

# And what the recursion left alone says nothing.
Ping->ping->nope;
