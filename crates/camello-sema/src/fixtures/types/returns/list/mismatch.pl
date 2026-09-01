use strict;
use warnings;

package Row;
use Moose;
has id => (is => 'ro', isa => 'Int');

package Store;

# The other half of ANNOT-7a: a `return` is checked against the list half of
# the annotation as well as the scalar half. A length that does not agree is
# reported the way a slot that does not agree is — the annotation is a comment
# either way, and a comment does not fail a build (ANNOT-7a).

# Returns: (Str, Int)
sub right { return ('a', 1) }

# Returns: (Str, Int)
sub too_many { return ('a', 1, 2) }
#~ warning return-mismatch: hands back 3 values where `Returns: (Str, Int)` names 2

# Returns: (Str, Int)
sub too_few { return ('a') }
#~ warning return-mismatch: hands back 1 value where `Returns: (Str, Int)` names 2

# A slot whose type does not agree follows the rule the scalar half does.
# Returns: (Str, Int)
sub wrong_slot { return ('a', []) }
#~ warning return-mismatch: `ArrayRef[Unknown]` returned where `Returns: (Str, Int)` names `Int`

# `return @rows` against `(Row ...)` is checked by element, since the length
# is not one anybody counted.
# Returns: (InstanceOf['Row'] ...)
sub rows {
    my @rows = (Row->new(id => 1));
    return @rows;
}

# Returns: (InstanceOf['Row'] ...)
sub wrong_element {
    my @numbers = (1, 2);
    return @numbers;
    #~ warning return-mismatch: `Int` returned where `Returns: (InstanceOf['Row'] ...)` names `InstanceOf['Row']`
}

# What the arguments alone do not say is silent: a call's shape is the
# callee's business, and this reading walks nothing a second time.
# Returns: (Str, Int)
sub through { return right() }
