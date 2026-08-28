use strict;
use warnings;
use Moose;

has name  => (is => 'ro', isa => 'Str', required => 1);
has count => (is => 'ro', isa => 'Int');
has items => (is => 'ro', isa => ArrayRef[Str]);

package Widget;
use Moose;
has label => (is => 'ro', isa => 'Str');
sub render { return 1 }

package main;

# A constructor takes a Dict of the attributes it declares.
my $widget = Widget->new(label => 'x');
Widget->new(label => [1]);      #~ error type-mismatch: declared `Str`
Widget->new(nope => 1);         #~ error unknown-key: declares no attribute `nope`

# A method the class declares resolves; one it does not is a warning, because
# the class may be right and the program wrong about what it holds.
$widget->render;
$widget->label;
$widget->missing;               #~ warning unknown-method: declares no method `missing`

# A slot the class requires and the call does not pass: `new` dies on it, so
# both sides are written down and it is an error. Named once, against the call,
# because what has to be fixed is the argument list.
package Required;
use Moose;
has must  => (is => 'ro', isa => 'Str', required => 1);
has spare => (is => 'ro', isa => 'Str', required => 1, default => 'x');
has maybe => (is => 'ro', isa => 'Str');

package Softer;
use Moose;
extends 'Required';
# Restating an inherited attribute to fill it in: the parent's `required => 1`
# is no longer the last word on it.
has '+must' => (default => 'y');

package main;

Required->new(must => 'x');
Required->new(maybe => 'x');    #~ error missing-argument: requires `must`
Required->new;                  #~ error missing-argument: requires `must`
Softer->new;

# An argument list this cannot read is one it says nothing about.
my %options = (must => 'x');
Required->new(%options);
Required->new({ must => 'x' });

# `args` dies on a missing name too, and `default` / `optional` are what say
# a name may be left out.
package Named;
use Smart::Args qw(args);
sub greet {
    args my $self,
         my $who   => 'Str',
         my $times => { isa => 'Int', default => 1 },
         my $loud  => { isa => 'Bool', optional => 1 };
    return "$who $times $loud";
}

# The same rule written `+{ ... }`, which is how a writer keeps perl from
# reading the brace as a block. The `+` says nothing about the value, so it is
# still a `default` and an `optional`. `optional => 0` is the one spelling
# that says the opposite.
sub shout {
    args my $self,
         my $who   => 'Str',
         my $times => +{ isa => 'Int', default => 1 },
         my $loud  => +{ isa => 'Bool', optional => 1 },
         my $tag   => +{ isa => 'Str', optional => 0 };
    return "$who $times $loud $tag";
}

package main;

my $named = bless {}, 'Named';
$named->greet(who => 'x');
$named->greet(times => 2);      #~ error missing-argument: requires `who`
$named->shout(who => 'x', tag => 'y');
$named->shout(who => 'x');      #~ error missing-argument: requires `tag`

# Smart::Args reads the rule before the type: a parameter that may be left out
# may also be *passed* `undef`, and the module returns it without ever asking
# the constraint. So the call below is a program that runs, and the one under
# it — the same `undef` against a name that has to be there — is not.
$named->greet(who => 'x', loud => undef);
$named->greet(who => undef);    #~ error type-mismatch: `Undef` passed to `who`

# `Unknown` propagates: an operation on something nobody typed says nothing.
my $opaque = get_it();
$opaque->anything_at_all;
sub get_it { return }

# A hand-written `new` in a class the run read through is an instance of that
# class — which is what gives a plain `bless` class any types at all.
package Counter;
sub new { my ($class, $start) = @_; return bless { count => $start }, $class }
sub add ($self, $amount) { $self->{count} += $amount; return $self }

package main;

my $counter = Counter->new(0);
$counter->add(1);
$counter->reset;                #~ warning unknown-method: declares no method `reset`
$counter->add(1, 2, 3);         #~ error arity: takes at most 2 arguments including its invocant; 4 passed
Counter->new(0)->add(1);

# A class the run never read keeps its `new` opaque, so nothing follows from it.
my $foreign = Somewhere::Else->new;
$foreign->anything_at_all;

# A `new` that borrows its parent's and blesses the result is still a
# constructor; a `new` that hands back whatever another class built is a
# factory, and calling its answer one of this class would make every method
# after it missing (INFER-2g).
package Sub2;
our @ISA = ('Counter');
sub new {
    my ($class, @rest) = @_;
    my $self = $class->SUPER::new(@rest);
    return $self;
}

package Factory;
sub new {
    my ($class, $start) = @_;
    return Counter->new($start);
}

package main;

Sub2->new(0)->add(1);
Sub2->new(0)->reset;            #~ warning unknown-method: declares no method `reset`
Factory->new(0)->anything_at_all;

# `SUPER::` is relative to the package the *line* is in, not to whatever the
# invocant turned out to be.
package Base;
sub new { my ($class) = @_; return bless {}, $class }
sub greet { return 'base' }

package Derived;
our @ISA = ('Base');
sub greet {
    my ($self) = @_;
    $self->SUPER::greet;
    $self->SUPER::nope;         #~ warning unknown-method: no parent of `Derived` declares
    return 'derived';
}

package main;

# An empty `()` on a sub whose body reads `@_` was a prototype, not a
# signature: perl still hands a method its invocant through one.
package Legacy2;
sub new { my ($class) = @_; return bless {}, $class }
sub header()
  { my $self = shift;
    return $self;
  }

package main;
Legacy2->new->header;

# A `bless` whose class cannot be read leaves nobody knowing what the value is
# — not still holding what it held before.
package Borrower;
sub build {
    my ($class, $how) = @_;
    my $self = Base->new;
    bless $self, $how;
    $self->anything_at_all;
    return $self;
}
