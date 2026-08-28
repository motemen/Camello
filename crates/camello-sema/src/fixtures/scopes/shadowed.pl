use strict;
use warnings;

my $depth = 1;
print $depth;

if ($depth) {
    my $depth = 2;              #~ warning shadowed-variable: `$depth`
    print $depth;
}

sub inner {
    my $depth = 3;              #~ warning shadowed-variable: `$depth`
    return $depth;
}

# A different sigil is a different variable.
my @depth = (1);
print "@depth";
