package Middle;
use strict;
use warnings;
use Deep;

# And this one cannot be read until `Deep::build` has been, which is the round
# after.
sub fetch { return Deep->build }

1;
