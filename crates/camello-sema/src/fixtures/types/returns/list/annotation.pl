use strict;
use warnings;

package Row;
use Moose;
has id => (is => 'ro', isa => 'Int');

package Store;

# `Returns: (A, B)` is finally used: a call in list context yields the
# annotated shape, and a sub that annotates only one context says nothing
# about the other.

# Returns: (ArrayRef[Str], Int)
sub pair { return ([], 1) }

# Returns: InstanceOf['Row']
# Returns: (ArrayRef[Str], Int)
sub both { return Row->new(id => 1) }

package main;

# The list half is what a list assignment reads, and the slots are in order.
my ($listed, $number) = Store->pair;
Row->new(id => $number);
Row->new(id => $listed);        #~ warning type-mismatch: `ArrayRef[Str]`

# The scalar half is what a scalar assignment reads. `pair` annotates none, so
# it says nothing there; `both` annotates both and each is read in its place.
my $nothing = Store->pair;
$nothing->whatever;
my $one = Store->both;
$one->nope;                     #~ warning unknown-method: `Row` declares no method `nope`
my ($also_listed) = Store->both;
Row->new(id => $also_listed);   #~ warning type-mismatch: `ArrayRef[Str]`
