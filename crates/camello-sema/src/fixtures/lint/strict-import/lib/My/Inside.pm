package My::Inside;
use strict;
use warnings;

# A `use` in an eval string ends with the eval: nothing reaches the caller.
sub import { eval q{ use List::Util; 1 } }

1;
