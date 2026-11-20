package My::Quiet;
use strict;
use warnings;
use parent -norequire, 'My::Script';

# Overrides the `import` that turns `strict` on with one that does nothing.
sub import { 1 }

1;
