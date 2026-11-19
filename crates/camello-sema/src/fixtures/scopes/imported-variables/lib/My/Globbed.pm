package My::Globbed;
use strict;
use warnings;
our %Config = (name => 'x');

sub import {
    my $caller = caller;
    no strict 'refs';
    *{ $caller . '::Config' } = \%Config;
}
1;
