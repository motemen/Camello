# The spellings a `Returns:` comment has and a declaration does not
# (`docs/types.md`, ANNOT-7f): `boolean` is `Bool` and `undef` is `Undef`. The
# comment is read by nothing but camello, so the word a writer reached for is
# the word; read as class names they were an `unknown-type` each and a
# `return-mismatch` on the value that satisfied them.
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

package Nil;
use Smart::Args qw(args);

# Returns: undef
sub nothing { return undef }

# Returns: Str | undef
sub maybe_str { return undef }

# Returns: undef
sub lies { return 'x' }
#~ warning return-mismatch: (`Str`) returned from a sub declared `Returns: Undef`

sub wants_str { args my $class, my $s => 'Str'; return $s }

package main;

Nil->wants_str(s => Nil->maybe_str);
Nil->wants_str(s => Nil->nothing);
#~ warning type-mismatch: declared `Str`

# A declaration is a string perl gives to a framework, and there an unknown
# name is a class name (TYPE-3) — `boolean.pm` blesses into one.
package Declared;
use Smart::Args qw(args);
sub takes_bool { args my $class, my $b => 'boolean'; return $b }
#~ info unknown-type: `boolean` is not known
sub takes_nil  { args my $class, my $n => 'undef';   return $n }
#~ info unknown-type: `undef` is not known
