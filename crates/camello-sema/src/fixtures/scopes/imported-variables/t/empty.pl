use strict;
use warnings;
use My::Empty;

# An `@EXPORT` written out as empty exports nothing, which is not the same
# as one nobody can read.
print $missing;    #~ error undeclared-variable: `$missing`
