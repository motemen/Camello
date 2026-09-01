use strict;
use warnings;

package Row;
use Moose;
has id => (is => 'ro', isa => 'Int');

package Store;

# A `Returns:` beside a body that says otherwise: the annotation is what the
# call site reads, and the body is what `return-mismatch` is about
# (`docs/types.md`, ANNOT-7a). An inferred return has nothing to contradict,
# so the diagnostic is still only ever against something written down.

# Returns: Maybe[InstanceOf['Row']]
sub find { return Row->new(id => 1) }

# Returns: Int
sub counted { return 'not a number' }
#~ warning return-mismatch: (`Str`) returned from a sub declared `Returns: Int`

package main;

# The annotation is what the caller gets, `Maybe` and all — the body's plain
# `Row` does not narrow it.
Store->find->id;                #~ warning maybe-deref: may be undefined here
