use strict;
use warnings;

package Row;
use Moose;
has id => (is => 'ro', isa => 'Int');

package main;

# Returns: Maybe[InstanceOf['Row']]
sub find { return undef }

# A `Maybe` used with nothing having checked it.
my $bare = find();
print $bare->id;                #~ warning maybe-deref: may be undefined here

# Every one of these is a narrowing, and each is a fixture rather than a
# theorem (`docs/typecheck.md`, "Narrowing").
my $checked = find();
if (defined $checked) {
    print $checked->id;
}

my $truthy = find();
if ($truthy) {
    print $truthy->id;
}

my $guarded = find();
return unless defined $guarded;
print $guarded->id;

my $defaulted = find() // Row->new(id => 1);
print $defaulted->id;
