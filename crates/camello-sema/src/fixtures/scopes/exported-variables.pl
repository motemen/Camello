use strict;
use warnings;
use English;
use Config;
use Regexp::Common;
use POSIX qw($errno);

# A module that binds a variable from its own `import` is running code camello
# does not run, so what it binds is a table (`docs/types.md`, A.10).
print $PROGRAM_NAME;
print $Config{osname};
print "1.2.3.4" =~ /$RE{net}{IPv4}/;

# An import list that names a variable declares it, whatever the module.
print $errno;

# Nothing else the module is named in comes with it.
print $NOT_IN_ENGLISH;          #~ error undeclared-variable: `$NOT_IN_ENGLISH`
