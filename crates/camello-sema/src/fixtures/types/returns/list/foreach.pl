use strict;
use warnings;

package Row;
use Moose;
has id => (is => 'ro', isa => 'Int');

package Store;

# Returns: (InstanceOf['Row'] ...)
sub rows { return () }

# Returns: (InstanceOf['Row'], InstanceOf['Row'])
sub pair { return () }

package main;

# The header is list context, so a call in it gives the element type where
# reading its scalar type gave `Unknown`.
for my $row (Store->rows) {
    $row->nope;                 #~ warning unknown-method: `Row` declares no method `nope`
}

# A known length is joined the same way: every slot is a thing the loop
# variable may be, so a pair of the same class is that class.
for my $either (Store->pair) {
    $either->nope;              #~ warning unknown-method: `Row` declares no method `nope`
}

# And through an array bound from the same shape.
my @rows = Store->rows;
foreach my $row (@rows) {
    $row->nope;                 #~ warning unknown-method: `Row` declares no method `nope`
}
