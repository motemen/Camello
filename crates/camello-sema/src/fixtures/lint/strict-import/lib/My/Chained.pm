package My::Chained;
use strict;
use warnings;

sub import {
    require My::Pragma;
    My::Pragma->import;
}

1;
