use strict;
use warnings;

package Counter;

sub new ($class, $start = 0) {
    return bless { count => $start }, $class;
}

sub add ($self, $amount) {
    $self->{count} += $amount;
    return $self;
}

package main;

Counter->new;
Counter->new(1);
Counter->new(1, 2);             #~ error arity: takes at most 2 arguments including its invocant; 3 passed

# An invocant arrives through `->`, so calling a method as a function is a
# different shape and is not counted against its parameter list.
Counter::add(1, 2, 3);

# A sub whose first parameter is not named `$self` or `$class` is not read as
# a method, and a call through `->` still passes the invocant.
package Address;
sub build ($this, $text = undef) { return bless { text => $text }, $this }
package main;
Address->build;
Address->build('x');
Address->build('x', 'y');       #~ error arity: takes at most 2 arguments including its invocant; 3 passed

# A method reached through the *type* of its invocant is counted by the flow
# pass rather than by this one, so that it is not said twice.
my $counter = Counter->new;
$counter->add(1);
$counter->add(1, 2, 3);         #~ error arity: takes at most 2 arguments including its invocant; 4 passed

# perlsub, "Prototypes": "Method calls are not influenced by prototypes
# either". A bare `()` is a prototype where the signatures feature is off and a
# signature where it is on, and nothing here can tell which — so the invocant
# the call passes is either harmless or fatal, and neither is worth an `arity`
# error. It is said once, at `info`.
package Legacy;
sub PI() { return 3.14 }
sub new { my ($class) = @_; return bless {}, $class }

package main;

Legacy->PI;
#~ info ignored-prototype: `PI` is declared `()`, which a method call ignores
Legacy->PI();
#~ info ignored-prototype: `PI` is declared `()`, which a method call ignores

# Reached through the type of its invocant rather than through a bareword: the
# same call, and the same answer.
my $legacy = Legacy->new;
$legacy->PI;
#~ info ignored-prototype: `PI` is declared `()`, which a method call ignores

# A bareword call *is* influenced by the prototype, and perl checks it.
Legacy::PI();
Legacy::PI(1);                  #~ error arity: takes at most 0 arguments; 1 passed

# The other half of the guess: an empty `()` on a sub whose body reads `@_`
# cannot have been a signature — perl would have made the body unreachable —
# so it is a prototype for certain and nothing is said at all. That one is in
# `types/flow.pl`, where the method resolution it belongs to is.
