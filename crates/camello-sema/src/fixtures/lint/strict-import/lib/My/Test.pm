package My::Test;
use strict;
use warnings;

# A project module with a lowercase name is read like any other (LINT-5f).
sub import {
    require t;
    t->import;
}

1;
