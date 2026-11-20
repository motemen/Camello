package My::CycleA;
use strict;
use warnings;

sub import { My::CycleB->import }

1;
