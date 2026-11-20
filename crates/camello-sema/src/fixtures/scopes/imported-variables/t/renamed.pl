use strict;
use warnings;
use My::Renamed;

# The caller gets the glob's name, not the variable's.
print $PUBLIC;
print $internal;    #~ error undeclared-variable: `$internal`
