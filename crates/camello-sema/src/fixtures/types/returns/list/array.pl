use strict;
use warnings;

package Row;
use Moose;
has id => (is => 'ro', isa => 'Int');

package Store;

# Returns: (InstanceOf['Row'] ...)
sub rows { return () }

# Returns: (InstanceOf['Row'], InstanceOf['Row'])
sub two { return () }

package main;

# The array's element type is the join of the shape's members, which is what
# `foreach` and `\@a` read. `$rows[0]` is not one of them: an element names
# its container, and camello does not track a bare array's (INFER-5a).
my @rows = Store->rows;
for my $row (@rows) {
    $row->nope;                 #~ warning unknown-method: `Row` declares no method `nope`
}

my @both = Store->two;
my $held = \@both;
$held->[0]->nope;
#~ info maybe-deref: may be undefined here #~ warning unknown-method: `Row` declares no method `nope`

# `map` hands back whatever its block said, with `$_` bound to the element.
my @ids = map { $_->id } Store->rows;
print "@ids";
my @broken = map { $_->nope } Store->rows;
#~ warning unknown-method: `Row` declares no method `nope`
print "@broken";

# `grep`, `sort` and `reverse` hand back some of what they were given.
for my $kept (grep { $_->id } Store->rows) {
    $kept->nope;                #~ warning unknown-method: `Row` declares no method `nope`
}
for my $ordered (reverse Store->rows) {
    $ordered->nope;             #~ warning unknown-method: `Row` declares no method `nope`
}

# In scalar context an array is its count, which is what perl says.
my $count = @rows;
Row->new(id => $count);

# A hash built from a list is key/value pairs, and nothing here reads them.
my %by_id = Store->rows;
print $by_id{1};
