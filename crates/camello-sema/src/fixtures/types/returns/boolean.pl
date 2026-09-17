# `boolean` is `Bool` in a `Returns:` comment (`docs/types.md`, ANNOT-7f). The
# comment is read by nothing but camello, so the word a writer reached for is
# the word; read as a class name it was an `unknown-type` and a
# `return-mismatch` on the `1` that satisfied it.
use strict;
use warnings;

package Flag;
use Smart::Args qw(args);

# Returns: boolean
sub lower { return 1 }

# returns: Boolean
sub cap { return !!0 }

# Wherever a type goes, at any depth.
# Returns: ArrayRef[boolean]
sub many { return [] }

# Returns: (boolean, Str)
sub pair { return (1, 'x') }

# Returns: boolean | Undef
sub maybe_flag { return undef }

sub wants_bool { args my $class, my $b => 'Bool';    return $b }
sub wants_hash { args my $class, my $h => 'HashRef'; return $h }

package main;

Flag->wants_bool(b => Flag->lower);
Flag->wants_bool(b => Flag->cap);
Flag->wants_bool(b => Flag->maybe_flag);

# Still a `Bool`, so what a `Bool` contradicts it contradicts.
Flag->wants_hash(h => Flag->lower);
#~ warning type-mismatch: declared `HashRef[Any]`

# A declaration is a string perl gives to a framework, and there an unknown
# name is a class name (TYPE-3) — the one `boolean.pm` blesses into.
package Declared;
use Smart::Args qw(args);
sub takes { args my $class, my $b => 'boolean'; return $b }
#~ info unknown-type: `boolean` is not known
