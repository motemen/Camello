# `$invocant->&name(...)` calls the lexical sub `name` with `$invocant` passed
# as its first argument. The `&` is part of the call, not a sigil on a name to be
# looked up in a package, so what follows the arrow is a lexical binding — see
# `lexical_sub_declaration.pl` for the declaration side.
use v5.42;

my sub bar {
    my ($class, %args) = @_;
    return $args{baz} <= $args{qux} ? undef : 'no';
}

sub foo {
    my ($class, %args) = @_;

    my @acc;
    push @acc, $class->&bar(baz => 1, qux => 10);

    my $first = $class->&bar(
        baz => $args{baz},
        qux => $args{qux},
    );

    return (\@acc, $first);
}
