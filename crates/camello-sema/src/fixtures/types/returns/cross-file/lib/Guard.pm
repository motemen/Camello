package Guard;
use strict;
use warnings;
use Deep;

# An annotation in the middle of a chain shadows what its body says, and every
# caller above it reads the annotation (`docs/types.md`, ANNOT-7a). The body
# hands back a plain `Row`; what the chain carries is the `Maybe`.

# Returns: Maybe[InstanceOf['Row']]
sub maybe_row { return Deep->build }

1;
