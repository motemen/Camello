package Deep;
use strict;
use warnings;
use Row;

# The bottom of the chain. `Row` is another file's, so tier 1 — which runs
# inside the declaration pass, before any other file is in — cannot read this;
# the first round of tier 2 can.
sub build { return Row->new(id => 1) }

1;
