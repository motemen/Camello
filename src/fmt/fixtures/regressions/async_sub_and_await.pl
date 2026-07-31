# `Future::AsyncAwait` adds two keywords through a parser plugin: `async` in
# front of a `sub` declaration, and the named unary `await`.
use Future::AsyncAwait;

async sub foo {
    my ($bar) = @_;

    my $x = await $bar->baz(1);
    my $y = await $bar->baz(2);

    return $x + $y;
}

async sub qux {
    return 0;
}
