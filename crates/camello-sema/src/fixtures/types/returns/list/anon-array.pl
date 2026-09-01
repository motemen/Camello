use strict;
use warnings;

package Row;
use Moose;
has id => (is => 'ro', isa => 'Int');

package Store;

# Returns: (InstanceOf['Row'] ...)
sub rows { return (Row->new(id => 1)) }

package main;

# `[ $self->rows ]` is an `ArrayRef[Row]` rather than an `ArrayRef[Unknown]`:
# the elements of an anonymous array are one of the places list context is
# written down rather than guessed at. An index into it is a `Maybe`, because
# nothing says the array is that long.
my $held = [ Store->rows ];
$held->[0]->nope;
#~ warning maybe-deref: may be undefined here #~ warning unknown-method: `Row` declares no method `nope`

# Flattened: a plural element contributes its members and not a reference,
# and a union of two classes resolves to neither.
my $mixed = [ 1, Store->rows ];
$mixed->[0]->nope;
#~ warning maybe-deref: may be undefined here
