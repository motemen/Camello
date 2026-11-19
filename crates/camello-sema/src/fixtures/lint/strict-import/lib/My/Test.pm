package My::Test;
use strict;
use warnings;

sub import {
    require My::Pragma;
    My::Pragma->import;
}

1;
