use strict;
use warnings;
use experimental 'class';
use Smart::Args::TypeTiny qw(args);

# perl gives a `method` its invocant and keeps it out of `@_`, so nothing in
# the declaration names one — while every call still passes one.
class Counter;

# A `field` declares its name for every `method` of the class. One with an
# attribute is read from outside the body — `:param` by the constructor,
# `:reader` by the accessor perl generates — so nothing is said about a body
# that does not read it; one without an attribute is a lexical like any other
# (`docs/types.md`, DIAG-2a).
field $count :param;
field $unused;
#~ info unused-variable: `$unused` is declared and never read

method value() {
    return $count;
}

method add($by) {
    return $count + $by;
}

package main;
use Smart::Args::TypeTiny qw(args);

sub use_it {
    args my $counter => 'Counter';

    # The invocant the signature never named is the one the call passes.
    $counter->value;
    $counter->add(1);
    $counter->add(1, 2);
    #~ error arity: takes at most 2 arguments including its invocant; 3 passed
    return;
}

# The block form, and the other two sigils a field may have.
class Point {
    field $x :param :reader;
    field $y :param = 0;
    field @history;
    field %seen;

    method describe() {
        push @history, $x;
        $seen{$x} = 1;
        return "$x,$y " . scalar(keys %seen) . scalar(@history);
    }

    method typo() {
        return $nope;
        #~ error undeclared-variable: `$nope`
    }
}
