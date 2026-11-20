use strict;
use warnings;

# `use vars` in another file declared it for the package, not for that file.
package My::Settings;
print $CONFIG[0];
print $NOT_DECLARED;    #~ error undeclared-variable: `$NOT_DECLARED`
