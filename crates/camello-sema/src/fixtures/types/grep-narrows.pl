# `grep` hands back the elements whose condition held, so the condition is read
# as a narrowing of `$_` and what survives it is the element type
# (`docs/types.md`, NARROW-7).
use strict;
use warnings;

package Row;
sub new { my ($class) = @_; return bless {}, $class }
sub id  { 1 }

package main;

# Returns: (InstanceOf['Row'] | Undef ...)
sub rows { return () }

# Nothing filtered it, so an element may still be `undef`.
for my $row (rows()) {
    print $row->id;             #~ warning maybe-deref: `$row` may be undefined here
}

# The truth test itself is a narrowing (NARROW-2), and `grep` is where it says
# something about a whole list.
for my $row (grep { $_ } rows()) {
    print $row->id;
}

# The same, written the two other ways.
for my $row (grep { defined $_ } rows()) {
    print $row->id;
}
for my $row (grep { defined } rows()) {
    print $row->id;
}
for my $row (grep $_, rows()) {
    print $row->id;
}

# A condition that says nothing about `$_` narrows nothing.
for my $row (grep { 1 } rows()) {
    print $row->id;             #~ warning maybe-deref: `$row` may be undefined here
}

# `!defined` keeps exactly the undefs; the narrowing list has no rule that says
# so, and claiming one would be claiming more than NARROW does.
for my $row (grep { !defined $_ } rows()) {
    print $row->id;             #~ warning maybe-deref: `$row` may be undefined here
}

# `map` hands back what its block said, which is a different question.
for my $row (map { $_ } rows()) {
    print $row->id;             #~ warning maybe-deref: `$row` may be undefined here
}
