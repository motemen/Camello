package My::Deep;
use strict;
use warnings;

sub import {
    require My::Test;
    My::Test->import;
}

1;
