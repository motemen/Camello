# The shapes are `Class-Tiny-1.008/t/attributes.t` and `t/subclass.t`, and the
# synopsis of `Class::Tiny::Antlers`.
use strict;
use warnings;

package Foo;
use Class::Tiny qw( name email ), {
    created => sub { time },
    size    => 0,
};

sub label {
    my $self = shift;
    return $self->name;
}

# `use Class::Tiny` with nothing after it is still what puts
# `Class::Tiny::Object` in `@ISA`, so there is a `new` (ANNOT-13b).
package Bare;
use Class::Tiny;

package Child;
use parent -norequire, 'Foo';
use Class::Tiny qw( extra );

# The one spelling that exports `has`, which carries types (ANNOT-13d).
package Antlers;
use Class::Tiny::Antlers;
has count => (is => 'ro', isa => 'Int');

package main;

my $foo = Foo->new(name => 'x', email => 'x@example.com');
print $foo->name, $foo->created, $foo->size, $foo->label;
# Read-write, and nothing is required, so a slot may simply be left out.
$foo->email('y@example.com');
print Foo->new->size;
# The constructor blesses what it was handed, so an unknown key is a warning
# rather than a contradiction (ANNOT-13c).
print Foo->new(whatever => 1)->name;    #~ warning unknown-key: `whatever`
print $foo->nope;                       #~ warning unknown-method: `nope`

print Bare->new;

my $child = Child->new(name => 'y', extra => 2);
print $child->extra, $child->name;

print Antlers->new(count => 3)->count;
print Antlers->new(count => 'three')->count;    #~ error type-mismatch: `count`
