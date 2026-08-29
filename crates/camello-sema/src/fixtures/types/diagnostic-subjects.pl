# A diagnostic names the code it is about, not only the type
# (`docs/types.md`, DIAG-0a). The subject is the expression as written, which
# is a variable only some of the time.
use strict;
use warnings;

package Row;
sub new { my ($class) = @_; return bless {}, $class }
sub id  { 1 }

package main;

# Returns: Maybe[InstanceOf['Row']]
sub find { return undef }

# Returns: Maybe[HashRef[Maybe[Str]]]
sub cfg { return undef }

my $row = find();
print $row->id;                 #~ warning maybe-deref: `$row` may be undefined here

# The receiver of a `->` is whatever was written, subscripts included.
my $held = { row => find(), deep => { inner => find() } };
print $held->{row}->id;         #~ warning maybe-deref: `$held->{row}` may be undefined here
print $held->{deep}{inner}->id; #~ warning maybe-deref: `$held->{deep}{inner}` may be undefined here

# What a step dereferences is everything written before it, so the second step
# of a chain is about the first one's result and not about the base.
my $c = cfg();
print $c->{a};                  #~ warning maybe-deref: `$c` may be undefined here
print $c->{a}{b};               #~ warning maybe-deref: `$c` may be undefined here
                                #~ warning maybe-deref: `$c->{a}` may be undefined here
