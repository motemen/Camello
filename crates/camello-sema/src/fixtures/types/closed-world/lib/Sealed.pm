package Sealed;
use strict;
use warnings;
# Recognised by name, so not a hole: the checker knows what this installs
# better than reading the file would tell it (`docs/types.md`, DIAG-7a).
use Class::Accessor::Lite (new => 1, ro => ['name']);
use Neighbour;

sub greet { 1 }

1;
