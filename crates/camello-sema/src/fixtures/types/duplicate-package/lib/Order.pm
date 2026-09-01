package Order;
use strict;
use warnings;

use Smart::Args::TypeTiny qw(args args_pos);

sub want_int {
    args my $n => 'Int';
    return $n;
}

sub charge {
    args_pos my $class => 'ClassName',
        my $label => 'Str';
    # The annotation above is what types `$label`, because it is the one in
    # this file. Typed from the namesake under `legacy/`, the parameter would
    # be `Any` and this line would be silent.
    Order->want_int(n => $label);
    #~ warning type-mismatch: declared `Int`
    return $label;
}

1;
