package Generator;
use strict;
use warnings;

# The package written into is worked out from `caller`, so what this puts
# there is not something the pass can list (`docs/types.md`, DIAG-7a).
sub import {
    my $caller = caller;
    no strict 'refs';
    *{"${caller}::installed_by_import"} = sub { 1 };
}

1;
