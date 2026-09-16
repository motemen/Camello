# Test newlines written against `->` (NEWLINE-4)

# the whole chain broken, invocant alone on its line
my $rows = $schema
    ->resultset('Artist')
    ->search({ name => $name })
    ->all;

# broken from the second call on
my $sum = $collection->items
    ->filter(sub { $_->active })
    ->reduce(0);

# a statement, not a declaration
$logger->open
    ->write('hello')
    ->close;

# a call in the middle holding an argument list written across lines: the
# chain after its closing paren keeps its own lines
my $built = $builder->start('x')
    ->with(
        k => {
            k1 => 'v1',
            k2 => 'v2',
        },
    )
    ->then('k1' => 0)
    ->also('k2' => { k2a => [ 1, 3 ] })
    ->plus('k3' => $v)
    ->end($n + 1);

# the break written after the arrow instead of before it
my $old_style = $frontend->
    myprint('a')->
    mywarn('b');

# subscripts and a code dereference are arrows too
my $language = ($self->{parsed})
    ->{language};
my $value = $self->{closures}
    ->{$method}
    ->(@_);

# a chain written on one line stays on one line
my $flat = $obj->one->two->three;
