package My::Globbed;
use strict;
use warnings;
my $prefix = __PACKAGE__ . '::';
*{ $prefix . 'import' } = sub { warnings->import; $^H |= 1538 };

1;
