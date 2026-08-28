# The shapes are `Type-Tiny-2.008001/t/lib/DemoLib.pm` and `t/lib/BiggerLib.pm`,
# the type libraries Type::Tiny's own test suite is written against. The point
# of copying them is the calling conventions: a quoted parent, a `where` block
# with no comma before it, a trailing `message`, and a `class_type` whose name
# is the class.
use strict;
use warnings;

package DemoLib;
use Type::Utils;
use Type::Library -base;

declare "String",
    where { not ref $_ }
    message { "is not a string" };

declare "Integer",
    as "Int",
    where { $_ =~ /\A[0-9]+\z/ };

package BiggerLib;
use Type::Utils qw(:all);
use Type::Library -base;

declare "SmallInteger",
    as "Integer",
    where { $_ < 10 }
    message { "$_ is too big" };

package Quux;
sub new { bless {}, shift }

package Foo::Bar;
sub new { bless {}, shift }
sub foo { 1 }
sub bar { 2 }

package Types;
use Type::Utils qw(:all);
use Type::Library -base;

role_type "DoesQuux", { role => "Quux" };
class_type "FooBar", { class => "Foo::Bar" };
# No hashref: the name is the class.
class_type "Foo::Bar";
duck_type "CanFooBar", [qw/ foo bar /];
enum "Colour", [qw( red green blue )];
union "Id", [ Int, Str ];

package main;
use Smart::Args::TypeTiny qw(args);

# Every one of them resolves, so the bareword is never read as a class name
# nothing declares — which is what `unknown-type` would have said.
sub takes {
    args my $small  => SmallInteger,
         my $handle => FooBar,
         my $ducky  => CanFooBar,
         my $colour => Colour,
         my $id     => Id;
    return "$small $handle $ducky $colour $id";
}

takes(small => 1, handle => Foo::Bar->new, ducky => Quux->new, colour => 'red', id => 1);
# `SmallInteger` is `Integer` is `Int`, all the way down.
takes(small => 'x', handle => Foo::Bar->new, ducky => Quux->new, colour => 'red', id => 1);
#~ error type-mismatch: `small`
