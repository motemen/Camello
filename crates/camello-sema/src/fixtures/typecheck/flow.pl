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
