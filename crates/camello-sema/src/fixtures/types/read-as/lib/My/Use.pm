package My::Use;
use strict;
use warnings;
use My::Point;

# The wrapper's own file says nothing, so without the setting this class has
# no attributes, no `new` and no accessors — and every line below is silent.
sub run {
    my $point = My::Point->new(x => 1, y => 2);
    my $wrong = My::Point->new(x => 'no', y => 2);
    #~ error type-mismatch: (`Str`) passed to `x`, which is declared `Int`
    return $point->x + $point->y + $wrong->z;
    #~ warning unknown-method: `My::Point` declares no method `z`
}

1;
