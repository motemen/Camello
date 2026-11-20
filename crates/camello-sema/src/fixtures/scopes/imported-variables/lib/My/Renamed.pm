package My::Renamed;
use strict;
use warnings;
our $internal = 1;

sub import {
    my $caller = caller;
    no strict 'refs';
    *{"${caller}::PUBLIC"} = \$internal;
}
1;
