package My::Pragma;
use strict;
use warnings;

sub import {
    strict->import;
    warnings->import;
}

1;
