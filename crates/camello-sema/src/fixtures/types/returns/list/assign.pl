use strict;
use warnings;

package Row;
use Moose;
has id => (is => 'ro', isa => 'Int');

package Store;

# Returns: (InstanceOf['Row'], Str)
sub pair { return (Row->new(id => 1), 'x') }

# Returns: (InstanceOf['Row'] ...)
sub rows { return () }

# Returns: InstanceOf['Row']
sub one { return Row->new(id => 1) }

package main;

# A known length hands slot `i` to target `i`.
my ($row, $name) = Store->pair;
print $row->id;
print $name;
$row->nope;                     #~ warning unknown-method: `Row` declares no method `nope`

# A target past the end of a known length is `undef`, which is what perl
# leaves there — and `undef` on its own is silent.
my ($first, $second, $third) = Store->pair;
print $first->id, $second, $third;
$third->id;

# `(Row ...)` does not say how many, so a single target may be the one that
# was not there.
my ($maybe) = Store->rows;
$maybe->id;                     #~ info maybe-deref: may be undefined here

# A `@rest` takes everything from its position on.
my ($head, @tail) = Store->pair;
print scalar @tail;
$head->nope;                    #~ warning unknown-method: `Row` declares no method `nope`

# A sub that says nothing about list context sinks this half and not the
# scalar one: one value in scalar context says nothing about a shape.
my ($nothing_known) = Store->one;
$nothing_known->whatever;

# And in scalar context the same call is the scalar half.
my $scalar = Store->one;
$scalar->nope;                  #~ warning unknown-method: `Row` declares no method `nope`
