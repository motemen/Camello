# `(Value, Undef) | (Undef, Error)` — the ok-or-error idiom, whose two slots
# are correlated: joining them slot-wise into `(Value|Undef, Error|Undef)`
# throws away the only thing the shape says (`docs/return-inference.md`, "The
# shape").
use strict;
use warnings;

package main;

# Returns: (Str, Undef) | (Undef, Int)
sub left  { return ('a', undef) }

# Returns: (Str, Undef) | (Undef, Int)
sub right { return (undef, 1) }

# Neither alternative: `Int` in the slot the annotation gives `Str` on one side
# and `Undef` on the other.
# Returns: (Str, Undef) | (Undef, Int)
sub neither { return (1, 'a') }
#~ warning return-mismatch: which is none of the shapes

# The length is written down on both sides, so it is an error however many
# alternatives there are.
# Returns: (Str, Undef) | (Undef, Int)
sub too_many { return ('a', undef, 1) }
#~ error return-mismatch: hands back 3 values

# One slot has nothing to be correlated with, so an alternation of them is the
# union of them and is shown that way.
# Returns: (Str) | (Int)
sub one_slot { return 'a' }

# The alternatives all have one length.
# Returns: (Str, Int) | (Str)
#~ info bad-annotation: the alternatives of a list shape all have the same number of slots
sub ragged { return ('a', 1) }

# A scalar union is still a scalar union: the `|` there belongs to the type
# language and not to this notation.
# Returns: Str | Undef
sub scalar_union { return 'a' }
