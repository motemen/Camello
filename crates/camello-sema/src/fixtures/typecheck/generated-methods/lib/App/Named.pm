package App::Named;
use strict;
use warnings;

use Exporter 'import';

# A list this *can* read is a list of names, and each is a sub of every
# package that imports it (`docs/types.md`, METHOD-6).
our @EXPORT = qw(greet &shout $VERSION);

sub greet { return 'hello' }
sub shout { return 'HELLO' }

1;
