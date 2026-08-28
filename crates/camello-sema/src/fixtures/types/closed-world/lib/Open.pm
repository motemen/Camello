package Open;
use strict;
use warnings;
# Nothing on the search path answers to this, and a module installs subs into
# its importer — so what `Open` has is a floor, not the whole set.
use Some::Module::Nobody::Has;

sub greet { 1 }

1;
