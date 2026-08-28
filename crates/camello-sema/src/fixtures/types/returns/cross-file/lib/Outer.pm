package Outer;
use strict;
use warnings;
use Guard;

sub via_guard { return Guard->maybe_row }

1;
