use strict;
use warnings;

my $depth = 1;
print $depth;

if ($depth) {
    my $depth = 2;              #~ info shadowed-variable: `$depth`
    print $depth;
}

sub inner {
    my $depth = 3;              #~ info shadowed-variable: `$depth`
    return $depth;
}

# A different sigil is a different variable.
my @depth = (1);
print "@depth";
