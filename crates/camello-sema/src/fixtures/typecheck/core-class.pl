use experimental 'class';
use Smart::Args::TypeTiny qw(args);

# perl gives a `method` its invocant and keeps it out of `@_`, so nothing in
# the declaration names one — while every call still passes one.
#
# No `use strict` here on purpose: the scope pass does not yet read `field` as
# a declaration, so a class under strict reports every one of its fields as
# undeclared. That is a gap of its own and not what this fixture is about.
class Counter;

field $count :param;

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
