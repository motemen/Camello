use strict;
use warnings;

my $c = 1;
my $x = compute() if $c;      #~ warning conditional-declaration
my ($a1, $b1) = (1, 2) unless $c;  #~ warning conditional-declaration
my $r = Foo->bar(             #~ warning conditional-declaration
    a => 1,
) if $ENV{RUN};
our $y = 1 if $c;
local $ENV{X} = 1 if $c;
my $z = $c ? 1 : 2;
if ($c) { my $w = 1; print $w }
sub compute { 1 }
