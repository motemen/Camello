package Order;
use strict;
use warnings;

use Smart::Args::TypeTiny qw(args_pos);

sub charge {
    args_pos my $class => 'ClassName',
        my $label => 'Str';
    return $label;
    #      ^ hover $label : Str
}

1;
