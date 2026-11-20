use strict;
use warnings;
use My::Vars qw(:all);

print $Offered{x}, $Tagged;
print $Unknown;    #~ error undeclared-variable: `$Unknown`
