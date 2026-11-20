use strict;
use warnings;
use feature 'state';

my $s .= "a";                 #~ warning augmented-assignment-in-declaration
my $n += 1;                   #~ warning augmented-assignment-in-declaration
my $d ||= 5;                  #~ warning augmented-assignment-in-declaration
local $ENV{PATH} .= ":/x";    #~ warning augmented-assignment-in-declaration
our $g .= "b";
sub counter { state $c += 1; state $once //= 5; $c + $once }
my $t = "a";
$t .= "b";
local $ENV{HOME} = "/tmp";
print $s, $n, $d, $t, counter();
