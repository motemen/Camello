# A comment written where the spacing rule puts nothing — before a `->`, or
# after an opening bracket that hugs the name in front of it — is kept all the
# same, and the line it lands on is a continuation of the expression around it.
# The gap had no separator at all, so nothing carried the break: the comment and
# everything under it came back at the statement's own level, in column zero of
# a statement indented four.
sub chain {
    my $foo = $bar->foo('foo')
        # foo
        ->bar('bar');
}

# The chain itself still closes up (`user_newlines.pl`), and perltidy answers
# this one the same way: it is the comment that has to go on a line of its own.
my $obj = $factory->create()->initialize()->configure();

frobnicate(
    # INITIALIZER: BEGIN block
    $first,
    $second,
);
