package My::Evaluated;
use strict;
use warnings;

sub import {
    my $target = caller;
    eval qq{package $target; use Test2::V0; 1} or die $@;
}

1;
