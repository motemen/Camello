package My::Handing;
use strict;
use warnings;
use parent -norequire, 'My::Script';

sub import {
    my $class = shift;
    $class->SUPER::import(@_);
}

1;
