package My::Relaxed;
use strict;
use warnings;
require Exporter;

sub import {
    no strict 'refs';
    my $caller = caller;
    *{"${caller}::relaxed"} = \&relaxed;
    Exporter->import;
}

sub relaxed { 1 }

1;
