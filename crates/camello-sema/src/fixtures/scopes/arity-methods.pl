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
