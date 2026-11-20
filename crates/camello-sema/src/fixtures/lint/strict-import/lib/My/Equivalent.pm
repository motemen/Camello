package My::Equivalent;
use strict;
use warnings;

sub import {
    my $setup = \&My::Setup::run;
    goto &$setup;
}

1;
