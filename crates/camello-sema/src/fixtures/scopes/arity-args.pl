use strict;
use warnings;
use Smart::Args;

# `args` names its parameters, so a call is `key => value` pairs and an odd
# count cannot be pairs at all. Smart::Args dies on that, so it is an error.
sub greet {
    args my $who   => 'Str',
         my $times => { isa => 'Int', default => 1 };
    return "$who $times";
}

greet(who => 'world');
greet(who => 'world', times => 2);
greet(who => 'world', 'times'); #~ error arity: neither a `key => value` pair
greet('world');                 #~ error arity: neither a `key => value` pair

# A value that could be a hash reference is one Smart::Args accepts, so
# nothing is said about it.
my $options = { who => 'world' };
greet($options);
greet(%$options);

sub at {
    args_pos my $index => 'Int';
    return $index;
}

at(0);
at();                           #~ error arity: takes at least 1 argument; 0 passed
at(0, 1);                       #~ error arity: takes at most 1 argument; 2 passed
