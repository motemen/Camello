package My::Named;
use strict;
use warnings;
our %settings = ();

sub import {
    my $caller = caller;
    no strict 'refs';
    for my $name (qw(Settings)) {
        *{"${caller}::$name"} = \%settings;
    }
}
1;
