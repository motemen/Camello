package MyApp::Types;
use strict;
use warnings;
# The DSL is one vocabulary that several distributions supply, so any of the
# `Type::` / `Types::` / `MooseX::Types` family is the import that could have
# provided it — here, the one the constants come from and nothing else.
use Types::Standard -types;
use Smart::Args::TypeTiny qw(args);

# What a project's own type library declares stands behind every annotation
# that names it: the bareword would otherwise read as a class name, and a
# class name nothing declares is `Unknown` and silent.
type Count   => as Int;
type Counts  => as ArrayRef [Count];
type Foo     => as Enum [ qw(foo) ];
type Bar     => as Enum [ qw(bar) ];
type FooBar  => as Foo | Bar;
subtype Name => as Str;
enum 'Colour', [qw(red green)];
class_type 'Handle', { class => 'MyApp::Handle' };
# The lattice has no intersection, so the name is `Unknown` — read, and silent.
intersection Both => [ Foo, Bar ];

# A name that stands for itself is a class after all, and a chain that closes
# on itself is not a type at all. Neither is a stack overflow.
type Loopy   => as Cyclic;
type Cyclic  => as Loopy;
type Missing => as Nope;
#~ info unknown-type: `Nope` is not known

sub counted {
    args my $n => Count, my $ns => Counts, my $which => FooBar, my $both => Both;
    return "$n @$ns $which $both";
}

counted(n => 1, ns => [1], which => 'foo', both => 1);
counted(n => 'x', ns => [1], which => 'foo', both => 1);
#~ error type-mismatch: `Str` passed to `n`
counted(n => 1, ns => 'x', which => 'foo', both => 1);
#~ error type-mismatch: `ns`

# Returns: Counts
sub counts { return [] }

package main;
