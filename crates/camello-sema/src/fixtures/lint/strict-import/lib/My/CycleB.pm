package My::CycleB;
use strict;
use warnings;

sub import { My::CycleA->import }

1;
