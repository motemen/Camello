package hinted;
use strict;
use warnings;

sub import { $^H |= 0x1 }

1;
