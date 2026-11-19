package My::Plain;
use strict;
use warnings;

sub import {
    my $class = shift;
    $class->export_to_level(1, @_);
}

1;
