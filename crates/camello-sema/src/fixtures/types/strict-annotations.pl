use strict;
use warnings;

# Under `--strict-annotations`, a public sub that says nothing about its own
# shape is worth being told about. It is `info`: a thing the user asked for,
# not a contradiction between two declared things.

sub unannotated { return 1 }    #~ info missing-annotation: is public and says nothing

sub with_a_signature ($count) { return $count }

# Returns: Int
sub with_a_return { return 1 }

# A leading underscore is the language-wide way of saying "not public".
sub _private { return 1 }

# A phase block and an all-caps name are perl's, not the program's interface.
sub DESTROY { return }
