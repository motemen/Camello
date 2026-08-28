use strict;
use warnings;

# perl dies on a signature mismatch, so a count against one is a contradiction
# between two declared things.
sub takes_two ($first, $second) {
    return $first + $second;
}

takes_two(1, 2);
takes_two(1);                   #~ error arity: takes at least 2 arguments; 1 passed
takes_two(1, 2, 3);             #~ error arity: takes at most 2 arguments; 3 passed

sub takes_one_or_two ($first, $second = 2) {
    return $first + $second;
}

takes_one_or_two(1);
takes_one_or_two(1, 2);
takes_one_or_two();             #~ error arity: takes at least 1 argument; 0 passed

sub takes_at_least_one ($first, @rest) {
    return $first + scalar @rest;
}

takes_at_least_one(1, 2, 3, 4);
takes_at_least_one();           #~ error arity: takes at least 1 argument; 0 passed

# Perl flattens, so a call with an array in it has no count at all and nothing
# is compared.
my @arguments = (1, 2);
takes_two(@arguments);
takes_two(also_unknown());
sub also_unknown { return (1, 2) }

# `my ($a, $b) = @_` is a shape, not a rule: perl fills what it has and
# ignores the rest, so a mismatch is a warning.
sub unpacks_two {
    my ($first, $second) = @_;
    return $first + $second;
}

unpacks_two(1, 2);
unpacks_two(1, 2, 3);           #~ warning arity: takes at most 2 arguments; 3 passed

# Passing fewer is what perl does with `undef`, and half the corpus declares
# four names and calls with two, so an unpacking list has no minimum.
unpacks_two(1);

sub shifts_two {
    my $first = shift;
    my $second = shift || 0;
    return $first + $second;
}

shifts_two(1);
shifts_two(1, 2, 3);            #~ warning arity: takes at most 2 arguments; 3 passed

# A body that reaches for `shift` past its leading run takes an argument the
# run did not name, so nothing is known about how many it takes.
sub shifts_later {
    my $first = shift;
    my %rest;
    $rest{second} = shift;
    return $first;
}

shifts_later(1, 2, 3);

# A sub that reads `@_` as a list has no parameter list to compare against.
sub reads_the_list {
    my ($first) = @_;
    return $first + scalar @_;
}

reads_the_list(1, 2, 3);

# A sub that never touches `@_` takes any number of arguments, and perl does
# not mind.
sub ignores_everything {
    return 42;
}

ignores_everything(1, 2, 3);
