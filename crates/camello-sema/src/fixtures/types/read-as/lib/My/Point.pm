package My::Point;
use strict;
use warnings;
use My::Accessors (
    new => 1,
    ro  => {
        x => 'Int',
        y => 'Int',
    },
);

1;
